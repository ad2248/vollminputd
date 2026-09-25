#include <QApplication>
#include <QByteArray>
#include <QCommandLineOption>
#include <QCommandLineParser>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLocalSocket>
#include <QMessageBox>
#include <QPushButton>
#include <QTimer>

int main(int argc, char *argv[]) {
    QApplication app(argc, argv);
    QCoreApplication::setApplicationName(QStringLiteral("vollminputd-panel"));

    QCommandLineParser parser;
    parser.setApplicationDescription(QStringLiteral("vollminputd 控制面板"));
    parser.addHelpOption();
    QCommandLineOption instanceOption(QStringList{QStringLiteral("instance")},
                                      QStringLiteral("守护进程实例名"),
                                      QStringLiteral("name"), QStringLiteral("default"));
    parser.addOption(instanceOption);
    parser.process(app);

    const QString instance = parser.value(instanceOption);
    if (instance.isEmpty() || instance.contains(QLatin1Char('/'))) {
        parser.showHelp(1);
    }
    const QString runtime = qEnvironmentVariable("XDG_RUNTIME_DIR");
    if (runtime.isEmpty()) {
        QMessageBox::critical(nullptr, QStringLiteral("无法启动控制面板"),
                              QStringLiteral("XDG_RUNTIME_DIR 未设置"));
        return 1;
    }
    const QString path = runtime + QStringLiteral("/vollminputd_") + instance + QStringLiteral(".sock");

    QPushButton button(QStringLiteral("TOGGLE"));
    button.setWindowTitle(QStringLiteral("vollminputd 控制面板"));
    button.setMinimumSize(220, 90);
    QLocalSocket socket;
    QTimer timer;
    timer.setSingleShot(true);
    QByteArray response;
    bool finished = false;

    const auto fail = [&](const QString &message) {
        if (finished) return;
        finished = true;
        timer.stop();
        socket.abort();
        button.setEnabled(true);
        QMessageBox::warning(&button, QStringLiteral("操作失败"), message);
    };

    QObject::connect(&button, &QPushButton::clicked, [&] {
        finished = false;
        response.clear();
        button.setEnabled(false);
        timer.start(10000);
        socket.connectToServer(path);
    });
    QObject::connect(&socket, &QLocalSocket::connected, [&] {
        socket.write(QJsonDocument(QJsonObject{{QStringLiteral("id"), 1},
                                               {QStringLiteral("command"), QStringLiteral("toggle")}})
                         .toJson(QJsonDocument::Compact) + '\n');
    });
    QObject::connect(&socket, &QLocalSocket::readyRead, [&] {
        response.append(socket.readAll());
        if (response.size() > 4096) {
            fail(QStringLiteral("守护进程响应过长"));
            return;
        }
        const int end = response.indexOf('\n');
        if (end < 0) return;
        QJsonParseError error;
        const QJsonDocument document = QJsonDocument::fromJson(response.left(end), &error);
        const QJsonObject result = document.object();
        if (error.error != QJsonParseError::NoError || result.value(QStringLiteral("id")).toInt(-1) != 1 ||
            !result.value(QStringLiteral("ok")).isBool()) {
            fail(QStringLiteral("守护进程返回无效响应"));
        } else if (!result.value(QStringLiteral("ok")).toBool()) {
            fail(result.value(QStringLiteral("error")).toString(QStringLiteral("未知错误")));
        } else {
            finished = true;
            timer.stop();
            socket.disconnectFromServer();
            button.setEnabled(true);
        }
    });
    QObject::connect(&socket, &QLocalSocket::errorOccurred, [&](QLocalSocket::LocalSocketError) {
        fail(QStringLiteral("无法连接守护进程：") + socket.errorString());
    });
    QObject::connect(&socket, &QLocalSocket::disconnected, [&] {
        if (!finished) fail(QStringLiteral("守护进程在响应前断开连接"));
    });
    QObject::connect(&timer, &QTimer::timeout, [&] {
        fail(QStringLiteral("等待守护进程响应超时"));
    });

    button.show();
    return app.exec();
}
