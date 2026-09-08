# CI 基础镜像发布手册

CI 基础镜像保存固定的 Arch 系统依赖和 `Cargo.lock` 对应的 vendored Rust
crates。镜像位于 `git.kals.top/kals/vollminputd-ci`，只能通过手动触发
`.gitea/workflows/publish-ci-image.yml` 发布。

## 安全边界

- 发布工作流使用本次 Job 的短期 `GITEA_TOKEN`，权限仅为代码只读和
  Packages 写入，不需要长期 Registry PAT；
- Registry 登录信息只写入 `mktemp` 创建的临时 auth file，Job 结束时删除；
- 只对已经审查、可由维护者信任的分支触发发布工作流。工作流会执行该分支的
  `Containerfile.base`，不得对来源不明的分支执行；
- 测试消费镜像时固定 Registry digest，不使用可变的 `latest` tag。

## 首次发布

发布工作流合并到 `main` 后执行：

```bash
tea actions workflows dispatch publish-ci-image.yml \
  --login kals --repo kals/VoiceInput --ref main --follow
```

日志最后会输出如下结果：

```text
Published git.kals.top/kals/vollminputd-ci:arch-<fingerprint>@sha256:<digest>
```

确认 Gitea Packages 中存在镜像：

```bash
tea api --login kals 'packages/kals?type=container&q=vollminputd-ci'
```

第一版镜像发布后，再把 `tests/integration/Containerfile` 的 `FROM` 改为日志中
的完整 digest，并让常规测试工作流登录私有 Registry。首次发布前不要删除
现有 `Containerfile` 中的本地依赖安装逻辑。

## 更新 Rust 依赖

1. 在功能分支更新 `Cargo.toml` 和 `Cargo.lock`；
2. 检查 `Containerfile.base`。只有 Rust/Arch/系统库变化时才修改它，不要为
   Cargo 依赖变化添加无意义改动；
3. 推送分支并由维护者确认其可信；
4. 对该分支手动触发发布工作流：

```bash
tea actions workflows dispatch publish-ci-image.yml \
  --login kals --repo kals/VoiceInput --ref <branch> --follow
```

5. 将输出的新 digest 写入 `tests/integration/Containerfile`；
6. 提交 digest 更新，并等待常规集成测试在断网构建容器中通过。

内容 tag 由 `Containerfile.base`、`Cargo.toml`、`Cargo.lock` 的哈希共同生成。
相同输入会得到相同 tag；任何依赖或基础镜像定义变化都会得到新 tag。

## 恢复与核验

Registry 数据丢失时，从可信分支重新 dispatch 工作流即可。发布后应确认：

- 镜像内 `/opt/vollminputd-vendor` 非空；
- builder 的 Cargo 配置将 crates.io 替换为该 vendor 目录并启用 offline；
- 工作流的 `Verify vendored dependencies offline` 在 `--network=none` 下通过；
- 常规构建日志不再出现 `Downloaded ...`；
- 63 个 Rust 测试、打包、offline E2E 和 live E2E 全部通过。
