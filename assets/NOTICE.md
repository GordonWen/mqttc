# 图标资产说明

`icon.svg` 是 2026-11 为本项目绘制的 512 px 矢量母版（蓝色圆角底 + 白色
“消息气泡 + 遥测脉冲”字形，寓意 MQTT 消息与调试观测），由项目作者自绘，
随本仓库以 MIT License 授权。

`icons/mqttc-*.png` 由 `icon.svg` 导出：16–256 的脉冲线按小尺寸观感加粗到约
32/512（相对母版更粗），512 保持母版线宽；16–48 缩小后仍保留可辨识的脉冲。

`icon.ico` 由上述 16–256 的 PNG 打包成多尺寸 Windows 图标，供 exe 资源使用；
`icon.png` 是 512 px 母版 PNG（与 `icons/mqttc-512.png` 相同），供 Linux 桌面入口、
macOS `.icns` 打包等使用。iced 运行时窗口图标内嵌 `icons/mqttc-256.png`。
