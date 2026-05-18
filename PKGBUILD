# Maintainer: Your Name <your.email@example.com>
pkgname=voice-input
pkgver=0.1.0
pkgrel=1
pkgdesc="Linux voice input method daemon for Wayland"
arch=('x86_64')
url="https://github.com/ad2248/VoiceInput"
license=('Apache')
depends=('wl-clipboard')
makedepends=('rust' 'cargo')
source=("$pkgname-$pkgver.tar.gz::$url/archive/v$pkgver.tar.gz")
sha256sums=('SKIP')

build() {
    cd "$srcdir/VoiceInput-$pkgver"
    cargo build --release
}

package() {
    cd "$srcdir/VoiceInput-$pkgver"
    install -Dm755 target/release/VoiceInput "$pkgdir/usr/bin/voice-input"
    install -Dm644 LICENSE.txt "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
}
