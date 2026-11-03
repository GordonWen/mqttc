# Requirements Document

Status: implemented

## Introduction

会话页左侧订阅栏常驻，占用消息区宽度。输入框沿用 Fluent 默认最小 160×32，短字段被撑开，文字在过高的框里看起来像居中。本需求：表单控件左对齐并跟 Theme 控件高度走；订阅表单只在订阅时展开，订完收起；已订阅 Topic 以花签显示在消息区。

## Glossary

- **System**: mqttc 桌面应用
- **Session Page**: 右侧会话页（`page` 为 1）
- **Subscribe Form**: Topic 输入、QoS 选择与确认订阅按钮组成的表单
- **Subscription Chip**: 一条已订阅 Topic 的紧凑花签，可筛选或取消订阅
- **Field**: 单行输入框，高度跟随 Theme Token，文字左对齐

## Requirements

### Requirement 1: 输入框尺寸与左对齐

**User Story:** AS 调试人员, I want 输入框高度和按钮一致、文字靠左, so that 短字段不会被撑开，字也不会看起来浮在框中间

#### Acceptance Criteria

1. WHEN Session Page、连接配置页、自动回复页、发布预设页展示单行输入框, the System SHALL 把输入框高度设为当前 Theme 的控件高度
2. WHILE 单行输入框可见, the System SHALL 将输入文字与占位文字按左对齐显示
3. WHEN 输入框所在列宽度小于 Fluent 默认最小宽度 160 像素, the System SHALL 允许该输入框收窄到列宽
4. WHILE 标签与输入框或下拉框同一行, the System SHALL 将标签与控件垂直居中对齐
5. WHEN Theme 的字号或密度变化, the System SHALL 按同一组 Theme Token 更新输入框高度与字号

### Requirement 2: 订阅表单按需显示

**User Story:** AS 调试人员, I want 只在订阅时看到订阅表单, so that 订完之后消息区不再被左侧栏占掉

#### Acceptance Criteria

1. WHILE Session Page 可见且 Subscribe Form 未打开, the System SHALL 不展示 Topic 输入与 QoS 选择
2. WHEN 用户在已连接状态下打开订阅入口, the System SHALL 展示 Subscribe Form
3. WHEN 订阅请求成功（含该 Topic 已在列表中）, the System SHALL 收起 Subscribe Form
4. WHEN 用户关闭 Subscribe Form, the System SHALL 收起表单并保留已填写的 Topic 与 QoS
5. IF 订阅失败或尚未连接, the System SHALL 保持 Subscribe Form 打开，并在日志中提示原因
6. WHEN 用户切换当前连接, the System SHALL 收起 Subscribe Form

### Requirement 3: 已订阅 Topic 以花签展示

**User Story:** AS 调试人员, I want 订完之后仍能看到、筛选和取消订阅, so that 表单收起后操作入口还在

#### Acceptance Criteria

1. WHILE 当前连接存在至少一条订阅, the System SHALL 在消息区以 Subscription Chip 展示每条 Topic
2. WHEN 用户点击一条 Subscription Chip, the System SHALL 按该 Topic 筛选消息；若该 Chip 已是当前筛选, the System SHALL 取消筛选
3. WHEN 用户点击 Subscription Chip 上的取消控件, the System SHALL 取消该条订阅
4. WHILE 某条订阅为当前筛选, the System SHALL 高亮对应 Subscription Chip
5. WHEN 当前连接没有任何订阅, the System SHALL 不展示 Chip 行
