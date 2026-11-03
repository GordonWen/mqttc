# Requirements Document

Status: implemented

## Introduction

设置页目前只能改皮肤和语言。窗口最小 1180×720，字号与间距写死在界面里，缩到更小或放到更大时，文字和控件不能跟着变。本需求在设置页增加字号与界面密度，选择立即作用到全窗口，并写入 `mqttc.json`。

## Glossary

- **System**: mqttc 桌面应用
- **Settings Page**: 左侧栏进入的设置页（`page` 为 2）
- **Font Size**: 界面基准字号，四档：小、中、大、特大，对应 11 / 12 / 14 / 16 像素。中档 12px 对齐 Windows 标题栏常用字号
- **UI Density**: 控件高度、内边距与间距的疏密，三档：紧凑、标准、宽松
- **Theme Token**: `Theme` 上由 Font Size 与 UI Density 算出的字号与长度，供各页使用
- **App Preference**: 写入 `mqttc.json` 根级的窗口级偏好，与连接列表并列

## Requirements

### Requirement 1: 设置页选择字号与密度

**User Story:** AS 调试人员, I want 在设置页改字号和疏密, so that 缩小或放大窗口时界面仍可读、控件仍排得开

#### Acceptance Criteria

1. WHEN 用户打开 Settings Page, the System SHALL 在皮肤与语言之外展示 Font Size 与 UI Density 两组选项
2. WHILE Settings Page 可见, the System SHALL 用当前语言显示「字号」「小」「中」「大」「特大」，以及「界面密度」「紧凑」「标准」「宽松」
3. WHEN 用户选择 Font Size, the System SHALL 立即按该档更新全窗口文字，并把该选择写入 `mqttc.json`
4. WHEN 用户选择 UI Density, the System SHALL 立即按该档更新全窗口内边距、间距与控件高度，并把该选择写入 `mqttc.json`
5. WHEN 界面语言为中文或英文, the System SHALL 用当前语言显示上述标签、选项与说明文案

### Requirement 2: 字号与密度作用到全窗口

**User Story:** AS 调试人员, I want 改一处就让连接列表、会话、配置、日志和表单一起变, so that 不会出现半边大字半边挤在一起

#### Acceptance Criteria

1. WHEN Font Size 为小、中、大或特大, the System SHALL 把窗口默认字号分别设为 11、12、14 或 16 像素，正文、标签、标题、等宽日志相对该基准保持原有级差
2. WHEN UI Density 为紧凑、标准或宽松, the System SHALL 把内边距、间距与控件高度分别按基准的 75%、90%、110% 计算
3. WHILE Font Size 或 UI Density 已选定, the System SHALL 在连接列表、连接配置、会话、设置、自动回复、发布预设与关于页使用同一组 Theme Token
4. WHEN Font Size 为小且 UI Density 为紧凑, the System SHALL 把窗口最小尺寸降到低于 1180×720，使窗口可以缩得更小
5. WHEN Font Size 为大或特大，或 UI Density 为宽松, the System SHALL 提高窗口最小尺寸，使放大后的文字与控件仍能完整排列

### Requirement 3: 偏好持久化

**User Story:** AS 调试人员, I want 下次打开仍是我选的字号和密度, so that 不用每次再调

#### Acceptance Criteria

1. WHEN System 把 App Preference 写入 `mqttc.json`, the System SHALL 写入 Font Size 键 `small` / `medium` / `large` / `xlarge`，以及 UI Density 键 `compact` / `standard` / `comfortable`
2. WHEN System 启动并读到上述字段, the System SHALL 按写入值恢复 Settings Page 选项与全窗口 Theme Token
3. WHEN System 启动并读到不含 Font Size 或 UI Density 的旧配置, the System SHALL 将 Font Size 视为中（12 像素）、UI Density 视为标准，并继续加载其余字段
4. IF Font Size 或 UI Density 的写入值无法识别, the System SHALL 分别改用中与标准，并继续加载其余字段
5. WHEN 用户关闭窗口, the System SHALL 把当前 Font Size 与 UI Density 随其它 App Preference 写入 `mqttc.json`
