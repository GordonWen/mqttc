# 需求实施计划

- [x] 1. 皮肤改为 System / Light / Dark / Night
  - [x] 1.1 改 `Skin` 枚举、索引、键与旧值映射
    - 将 `src/main.rs` 的 `Skin` 改为 `System | Light | Dark | Night`
    - 设置页索引 0/1/2/3 对应 system/light/dark/night
    - 加载键：`white`→Light，`black`→Dark，`blue`/`green`/`purple`→Night，未知→Dark
    - `default_skin()` 改为 `"system"`（Requirement 7）
    - `is_light` / `window_bg_rgb` / `caption_text_rgb` 跟三套主题走
  - [x] 1.2 改 `Theme.pick` 与色板 Token
    - `ui/app-window.slint` 的 `pick` 改为三参数 Dark / Light / Night
    - 按 MQTTX 色板填窗口底、面板、强调色 `#34c388`、收发气泡、SYS/ERR
    - Light 的 Nav Rail 跟工作区浅色：栏底 `#ffffff`、条目 `#f1f8fc`、文字 `#222b3b`
    - `Palette.color-scheme`：Light 为 light，Dark/Night 为 dark
  - [x] 1.3 改设置页皮肤下拉与 I18n
    - 选项改为跟随系统 / 浅色 / 深色 / 夜间（英文 System / Light / Dark / Night）
    - 去掉蓝绿紫选项（Requirement 7）
  - [x] 1.4 更新 `skin_index_and_key_round_trip`
    - 覆盖四套皮肤往返与旧键映射、`window_bg_rgb`

- [x] 2. 订阅列表显隐写入配置
  - [x] 2.1 `AppConfig` 增加 `subs_visible`
    - `src/config.rs` 根级字段，默认 true
    - `apply_prefs` / `sync_prefs` 与窗口属性同步（Requirement 4、8）
  - [x] 2.2 窗口属性 `subs-visible`
    - `AppWindow` 增加 `in-out property <bool> subs-visible: true`
    - 隐藏/显示时 `sync_prefs` + `persist`
  - [x] 2.3 缺省与往返测试
    - 旧文件缺字段为 true；true/false 读写往返

- [x] 3. 检查点 - 皮肤与配置编译通过
  - 确保 `cargo test --offline` 通过，如有疑问请询问用户

- [x] 4. Nav Rail 与次级入口
  - [x] 4.1 左栏宽度与 Light 浅色栏
    - 用 `Theme.nav-w` 替换写死宽度
    - Light 下 Nav Rail 跟工作区浅色，Dark/Night 跟窗口深色系（Requirement 1、2、7）
  - [x] 4.2 次级入口高亮当前页
    - 自动回复 / 发布预设 / 设置 / 关于按 `page` 使用 `selected-bg`
    - 间距与按钮高度走 Theme Token（Requirement 1、2）

- [x] 5. 会话工作区改为 MQTTX 骨架
  - [x] 5.1 中段左侧 Subscription List
    - 删除消息区顶部花签行
    - `subs-visible` 为 true 时在 Message Pane 左侧纵向列表：Topic、QoS、筛选、取消
    - 空列表仍显示容器与新建订阅入口（Requirement 4）
  - [x] 5.2 新建订阅表单移到列表底部
    - 「新建订阅」打开 `subscribe-open`
    - 成功后 Rust 收起表单，失败保持打开（Requirement 4）
  - [x] 5.3 列表隐藏与恢复
    - 列表顶部隐藏控件；隐藏后 Message Pane 占满中段
    - 顶栏「断开」后在列表隐藏时提供重新显示入口（Requirement 4）
  - [x] 5.4 顶栏与 Publish Dock 保持裁剪横滑
    - `Flickable` 只写 `width: 100%`，`min-width` 放外层 Rectangle
    - 窄窗口右侧编辑/连接/断开与保存/清空完整可见（Requirement 3、5、6）
  - [x] 5.5 Secondary Page 统一间距
    - 连接配置、设置、自动回复、发布预设、关于使用同一组 padding / 圆角 / Token（Requirement 1）

- [x] 6. 检查点 - 确保所有测试通过
  - 确保 `cargo test --offline` 通过，如有疑问请询问用户

- [x] 7. 文档与发布包
  - [x] 7.1 更新 USER_GUIDE / INTERFACES / INDEX / ARCHITECTURE* / README / CHANGELOG
    - 皮肤四档、订阅列表、`subs_visible`、默认跟随系统
  - [x] 7.2 编 Linux 与 Windows release 覆盖 dist
