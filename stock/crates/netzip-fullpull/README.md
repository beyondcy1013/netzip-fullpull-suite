# netzip-fullpull

Shared protocol documentation authority is under [`docs/`](docs/README.md).
Consumers must link that status rather than maintain copied decoder gap lists.

`netzip-fullpull` 由旧共享 crate 改名，位于 `/home/codes/stock/crate`。它的产品语义是
复刻 `quoteNetzipWine` 使用正式账号登录后的官方全推能力：认证后建立非 7709 的厂商数据
连接（当前证据为 5188），完成初始化，并持续接收、解码和恢复服务端推送。

全推不等于遍历全市场主动查询。7709 代码表、快照、K 线、F10 和财务查询属于兄弟包
`netzip-supplement` 的补数据职责。

当前迁移状态：

- crate 仍包含旧共享 crate 迁入的 7709/0547、K 线、F10、FIN 和
  `NativeSession` 兼容代码；这是尚待迁出的历史物理布局。
- 这些 7709 模块不构成官方全推实现，也不应继续扩展为本包的长期职责。
- 后续应把查询能力收敛到 `netzip-supplement`，本包只保留正式账号官方全推链。

当前兼容 API 仍供 `quoteNetzipRs` 和 `tdxRs` 使用。新增代码必须按目标边界落位：官方全推
进入本包，7709 查询进入 `netzip-supplement`。详细边界见
`/home/codes/stock/quoteNetzipRs/docs/capability-boundaries.md`。

`auth_7100` 现已并入本包，负责 6100/7100 探测、运行时凭据登录、服务器列表下载、登录结果
校验和官方 5188 端点选择。认证控制链只在当前会话中插入账号密码，错误和结果对象不携带凭据；
`Stock.字典` 作为 crate 资源随包发布。Windows 客户端通过相对路径依赖
`../../../crates/netzip-fullpull`，不依赖机器上的绝对路径。

`auth_7100::decode_stock_dictionary_netpacket` 提供完整厂商 `网络包` 的离线分析入口。
它先校验外层对象及声明长度，再使用内置字典解压；该 API 不建立网络连接，也不会把解出的
认证、控制或下载对象提升为全推行情。

## 5188 进度

`official_5188` 提供认证后 5188 长连接的应用帧边界和方向分类。它覆盖 Wine 抓包中已确认的
客户端初始化帧（线序 `0x3610/0x2d10/0x2a10/0x0710`）和服务端持续帧（线序 `0x2704/0x0d04/0x5404/0x2104/0x3e04`），
支持 TCP 分段重组并保留未知 payload 原文。5188 内层压缩对象、证券字段映射和回调等价性尚未完成，
因此该模块不能把帧计数当作行情覆盖，也不会回退到 7709 解码。

`Official5188Session` 负责连接已由 6100 登录结果选出的 endpoint、读写有界 TCP 数据并交给
重组器。它不负责凭据登录、服务器列表下载或未确认的初始化字节；产品层必须先完成认证并明确
传入 endpoint。

`Official5188Handshake` 只验证 Wine 样本的客户端帧形状（线序 3 个 `0x3610`、3 个 `0x2d10`，其后
可选 `0x0710/0x2a10`），不把形状验证误报为登录成功或行情覆盖。

当前证据层解析还包括：`Official5188BulkEnvelope` 校验 `3e04` 的
`12 + 5120` 字节包络；`Official5188SubscriptionEnvelope` 校验 `2a10`
的 `10 + 6*N` 订阅条目，并提供未解释的 little-endian 数值。两者均不
宣称内层字段已经完整还原。正式样本中的连续 `3e04` body 在内嵌 `1504`
对象头之后承载跨 block 的 zlib 流，已解出的前缀为 `.//update//stkinfo6.fin`；
它属于文件分发，不作为 `2704` 行情 baseline。

`Official5188DeltaEnvelope` 已按 Wine `0x44aa30` 的静态调用约定确认
`2704` 前缀为 `u16 record_count + u32 value_end_offset`。offset 从 payload
起点计数，因此 value 位流为 `payload[6..value_end_offset]`，其后为 index
位流；这只确认编码边界，尚不代表 311 字节内部行情记录已经完成字段映射。
index 位流现可恢复 `market/symbol_index/timestamp/uses_baseline`：其中
`uses_baseline` 是内部 `+0xde` marker 的兼容字段名，value pass 是否传入完整 baseline
由 value mask bit 0 决定；timestamp 在正式样本中落到抓包时刻附近，symbol index 与
market 一起用于 Wine 的代码表查找。
`Official5188CodeTable` 同时解析 `0104` 解压对象的 98 字节头和 68 字节记录，
提供 symbol index 到 ASCII 代码及 GBK 名称的有界映射；未命名的 25 字节记录尾保持原样。

`Official5188InternalRecord` 提供已由 Wine core 与 callback 配对确认的 311 字节内部记录
布局视图，可读取时间、开高低收整数、累计量/金额、盘口数组和 market/symbol index。新增的
`decode_official_5188_values` 使用已验证的 Wine token 表，从 `2704` value 位流恢复实验性
内部记录，并通过 `Official5188BaselineResolver` 注入 baseline；`Official5188MapBaselineResolver`
适用于离线回放。该 API 仍不代表 5188 业务发布或 Wine callback parity 已完成，盘口数组保留
厂商顺序（买盘倒序后接卖盘）。
`Official5188InternalRecord::to_public_quote` 提供显式的 callback-shaped 投影：调用者必须
传入同一 `0104` 代码表的六位代码、名称和正数 `price_scale`，输出 `wjf.oem_report.v5`
字段及十档盘口；未确认的 scale、market 或 code 会直接返回错误。
`Official5188CodeTableRecord::price_scale_hint` 暴露正式抓包中 `opaque_tail[1]` 的受限提示
（目前确认值为 `1/10/100`）；未知值返回 `None`，不会替代独立 parity 验证。
离线多帧回放可复用同一个 `Official5188MapBaselineResolver`，并在每帧成功后调用
`update_from_decoded`。Wine 的 `mask_class == 0x18` inner decoder 会在 tail token 与字节对齐
之前返回，但 outer `2704` loop 仍复制 metadata 并把 temporary record 回写到 global table，
因此 resolver 也会持久化该类记录。value mask bit 0 要求的初始化 baseline 缺失时 decoder
返回明确错误，不使用全零记录代替。
固定正式 PCAP 配合保留的 Wine core 已完成 15 个 `2704` 帧、2,102 条记录的独立 Python
复核：每条 `bit_start/bit_end` 与完整 311 字节记录均一致。core loader 接受正常 live slot，
也接受 `timestamp == 0` 但代码 metadata 与 reference price 已初始化的 slot；只有 identity
而没有代码/reference price 的弱空槽会被拒绝。该结果仍不覆盖 callback 字段转换和批次聚合，
production decoder gate 保持关闭。
