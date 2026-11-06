# ymp

macOS 终端里的 TUI 音乐播放器。指定一个本地目录，读取曲目、作者、专辑与内嵌封面，在支持的终端里通过
Kitty 图形协议显示封面图片，并附带一条 FFT 频谱。

```
╭─ ymp  ~/Documents/music/piano_BenTenz ─────────── 37 tracks · 1 album · 4:35:34 ─╮
│  1 A Journey                                              3:27  ┌──────────────┐  │
│  2 Akiba Pop the Future                                   8:09  │              │  │
│ ▶3 Ancient Temple                                          2:22  │  album art   │  │
│  4 Aoi Tori                                               5:32  │              │  │
│  5 Armored Ladybird                                       2:29  └──────────────┘  │
│  ...                                                            ▂▄▆█▆▄▂▁▂▄▆▇▆▄▂      │
│                                                                 Ancient Temple        │
│                                                                 3 / 37   2:22         │
│  1:13 ━━━━━━━━━━━━━━━━━━●───────────────────── 2:22   SEQ  vol  80%                  │
│  space 播放/暂停   n/p 切歌   ←/→ 快进   m 模式   / 搜索   ? 帮助   q 退出             │
╰──────────────────────────────────────────────────────────────────────────────────────╯
```

## 关于这个项目

- **由 AI 生成**：代码与文档由 AI 助手根据使用者提出的需求编写，人工负责需求、取舍与测试。
- **仅适配 macOS**：开发和验证都在 macOS（Apple Silicon）上进行，其他平台未经测试，不保证可用。
- **不提供支持**：没有精力处理 issue 与 PR。遇到问题或有额外需求，请自行 fork、修改、构建。
- **按现状提供**：MIT 许可，不附带任何形式的担保。

## 功能

- 指定目录递归扫描，也可直接给单个音频文件
- 四种播放模式：顺序 `seq`、倒序 `rev`、随机 `shuffle`、单曲循环 `one`
- 读取标题、作者、专辑、年份、音轨号、时长；专辑标签缺失时退回文件夹名
- 封面优先取内嵌图（ID3 APIC / FLAC picture / MP4 covr），没有则找同目录的 `cover.jpg`、`folder.png` 等
- 三种封面后端：Kitty 图形协议、iTerm2 内联图片、半块字符（`▀`，双像素/字符）
- 实时频谱：音频回调取样，1024 点 FFT，16 段对数频带
- 不依赖 ffmpeg：Symphonia 解码 + CoreAudio（cpal）输出

支持格式取决于 Symphonia：mp3 / flac / m4a(AAC) / ogg / opus / wav / aiff 等，不支持 ALAC。

## 安装

需要 Rust 工具链（`rustup` 或 Homebrew 的 `rust`）。

```bash
cd ~/Desktop/ymp
./install.sh          # cargo build --release，装到 ~/.local/bin/ymp
```

脚本做三件事：构建 release、复制到 `~/.local/bin`、清除 macOS 加在二进制上的
`com.apple.provenance` 扩展属性。

> 自行 `cp` 到其他位置后若运行报 `Killed: 9`（退出码 137），原因就是该属性：
> 执行 `xattr -c /path/to/ymp`。`cargo install --path .` 装进 `~/.cargo/bin` 也可能出现同样情况。
> 更换安装位置：`YMP_INSTALL_DIR=/usr/local/bin ./install.sh`。

## 用法

```bash
ymp                                   # 当前目录；扫描完成后停在界面，按 space 开始
ymp ~/Music/piano_BenTenz             # 指定目录
ymp ~/Music/piano_BenTenz --play      # 扫描完成后自动播放第一首
ymp "song.flac"                       # 单个文件：以其所在目录为播放列表并选中它
ymp ~/Music -m shuffle --volume 60    # 随机播放，音量 60
ymp --probe                           # 查看终端能力
```

界面内 `space` 播放/暂停，`?` 查看快捷键，`q` 退出。

## 播放模式

播放时维护一个播放队列（order），队列内容为当前列表的曲目顺序。“下一首”始终指队列中的下一项，
四种模式共用这套逻辑，区别只在入队方式。

| 模式 | 取值 | 入队方式 | 一首放完后 | 到达队列末尾 |
| --- | --- | --- | --- | --- |
| 顺序 | `seq` | 按列表从上到下 | 播队列下一首 | 停止 |
| 倒序 | `rev` | 列表整个反转 | 播队列下一首（列表中的上一首） | 停止 |
| 随机 | `shuffle` | 剩余曲目洗牌一次，当前曲固定首位 | 播洗牌后的下一首 | 停止 |
| 单曲循环 | `one` | 队列照常 | 重放同一首 | 不会到达 |

几点行为说明：

- `shuffle` 是洗牌一次，不是每首单独随机；因此一个列表内不会重复播到同一首，`p` 也能回到上一首。
  再次切换模式时会重新洗牌剩余曲目。
- 切换模式以当前曲目为起点重排队列。例如播到第 5 首时切到随机，第 5 首仍是当前曲，
  第 6 首之后重新洗牌。
- `one` 只影响“一首自然播完”的情况，手动按 `n` / `p` 仍然切歌。
- `seq` / `rev` / `shuffle` 到列表末尾即停止，不会自动回到开头；再按一次 `space` 从当前选中项重新开始。
- 队列是播放开始时的快照。搜索过滤后按 `enter`，队列就是过滤结果；之后清空搜索，队列不变。

命令行写法：

```bash
ymp ~/Music -m seq        # 默认值
ymp ~/Music -m rev
ymp ~/Music -m shuffle
ymp ~/Music -m one
ymp ~/Music --mode=one    # 长写法，等价
```

界面内对应按键为 `1` `2` `3` `4`，或用 `m` 依次循环。

## 命令行参数

`ymp [OPTIONS] [PATH]`

| 参数 | 取值 | 默认 | 作用 |
| --- | --- | --- | --- |
| `[PATH]` | 目录或音频文件 | `.` | 播放目标。目录递归扫描；文件则以其所在目录为列表并选中该曲 |
| `-m, --mode` | `seq` `rev` `shuffle` `one` | `seq` | 播放顺序 |
| `--volume` | 0-100 | `80` | 初始音量，超出范围会被拒绝 |
| `--play` | 开关 | 关 | 扫描完成后直接播放第一首 |
| `--no-recursive` | 开关 | 关 | 只扫描指定目录这一层 |
| `-g, --graphics` | `auto` `kitty` `iterm` `blocks` `off` | `auto` | 封面后端 |
| `--cell-aspect` | 浮点数，约 1.2~4.0 | 自动探测 | 单元格高宽比，探测失败时使用 |
| `--probe` | 开关 | — | 打印终端能力后退出，不进入界面 |
| `--lang` | `cn` `en` | `cn` | 界面语言，影响帮助、版本、提示与报错；`--language` 同义 |
| `-h` | | | 简洁说明，每个参数一行 |
| `--help` | | | 完整说明：取值、默认值、示例、退出码 |
| `-v` / `-V` | | | 一行版本号 |
| `--version` | | | 完整信息：版本、构建目标、解码与音频后端、终端要求、许可 |

补充说明：

- **`[PATH]`**：`~` 由 shell 展开。给文件时以其父目录为扫描根并选中该曲，便于 `ymp 某首歌.flac`。
- **`-m, --mode`**：与 `--mode` 等价，取值含义见上一节。
- **`--volume`**：100 为原始音量，运行时用 `+` / `-` 每次调 5。`-v` 保留给版本号，因此音量只有长参数。
- **`-g, --graphics`**：`auto` 先探测再决定；`kitty` / `iterm` / `blocks` 为强制指定；`off` 不画封面，
  列表占满宽度。终端能显示图片但 `auto` 未识别时，用 `-g kitty` 强制。
- **`--cell-aspect`**：单元格“高 ÷ 宽”的像素比，常见等宽字体在 2.0~2.2 之间。程序会通过 `CSI 16 t`
  查询真实单元格尺寸，查不到时按 2.1 估算；封面比例异常时用此参数校正，例如 `--cell-aspect 2.4`。
- **`--probe`**：排查封面问题的第一步，输出示例：

```
ymp 终端探测
  TERM              xterm-256color
  TERM_PROGRAM
  kitty 图形协议      是
  iterm2 内联图       否
  背景               深色
  单元格尺寸          18x38 px
```

## 配置

配置文件：`~/.config/ymp/config`（设置了 `XDG_CONFIG_HOME` 时改用 `$XDG_CONFIG_HOME/ymp/config`）。

```
# ymp preferences
lang = en
```

- 只切换语言、不启动界面：`ymp --lang en`。目录不存在会自动创建，写完后立即退出
- 不带 `--lang` 时读取该文件；文件不存在或没有 `lang` 项时使用中文
- `--lang` 与 `-h` / `--version` 同时出现时只影响本次输出，不写入文件
- 未知键会被忽略，旧版本读到新版本写的文件也不会报错

## 快捷键

| 键 | 作用 |
| --- | --- |
| `space` | 播放 / 暂停；未在播放时播放当前选中项 |
| `enter` / `l` | 播放选中曲目，并把当前列表（含搜索过滤）设为播放队列 |
| `n` / `p` | 下一首 / 上一首 |
| `Tab` / `Shift-Tab` | 同 `n` / `p` |
| `←` / `→` | 后退 / 前进 5 秒 |
| `,` / `.` | 同上，不离开主键区 |
| `j` `k` / `↑` `↓` | 移动选中项 |
| `d` `u` / `PgDn` `PgUp` | 翻 8 行 |
| `g` / `G` | 跳到列表首 / 尾 |
| `1` `2` `3` `4` | 顺序 / 倒序 / 随机 / 单曲循环 |
| `m` | 依次循环四种模式 |
| `+` / `-` | 音量 ±5 |
| `/` | 搜索（标题、作者、专辑、文件名）；回车以结果为队列，Esc 取消 |
| `r` | 清空封面缓存并重绘 |
| `?` | 帮助浮层，任意键关闭 |
| `q` / `Ctrl-C` | 退出，停止播放并还原屏幕 |

## 封面显示

终端显示图片没有统一协议，实现方式如下：

1. **探测**：启动时以非阻塞方式向 `/dev/tty` 发送查询
   - `ESC _ G i=31,s=1,v=1,a=q,t=d,f=24;AAAA ESC \` → 返回 `OK` 表示支持 Kitty graphics
   - `ESC ] 11;? BEL` → 背景色，用于选择深色或浅色主题
   - `ESC[16t` → 单元格像素尺寸，用于保持封面比例
2. **发送**：解码内嵌图，缩放到「列数 × 单元格宽 / 行数 × 单元格高」，编码为 PNG 后 base64 分块
   （每块不超过 4096 字节），以 `a=T,f=100,t=d,C=1,c=列,r=行,q=2` 写入光标位置
3. **重绘控制**：仅在换曲、尺寸变化或终端 resize 时重发图片，其余帧只重绘文字
4. **降级**：不支持 Kitty 时退回 iTerm2 的 `OSC 1337;File`，再不行用半块字符渲染

| `-g` 取值 | 实现 | 适用终端 |
| --- | --- | --- |
| `auto` | 探测结果优先，失败则字符画 | 默认 |
| `kitty` | APC 图形协议 | kitty / WezTerm / Ghostty / ELYTerminal 等 |
| `iterm` | OSC 1337 内联图片 | iTerm2 系 |
| `blocks` | 半块字符拼色块 | 任意终端，包括 Terminal.app |
| `off` | 不绘制 | 窄窗口、SSH 慢链路 |

探测不准时（部分终端不回应查询，或把 `TERM` 报成 `xterm-256color`）可直接强制：

```bash
ymp ~/Music -g kitty     # 强制 Kitty 协议
ymp ~/Music -g blocks    # 强制字符画
ymp --probe              # 查看探测结果
```

## 实现

```
src/
  main.rs        入口与终端初始化收尾
  cli.rs         参数定义、帮助与语言
  i18n.rs        中英文文案
  app.rs         状态机：播放队列、模式、按键、事件循环
  scan.rs        目录扫描 + lofty 读取标签与内嵌封面
  model.rs       Track / Library
  audio/
    mod.rs       播放线程 + 解码线程 + 有界环形缓冲
    decoder.rs   symphonia 解码 → 立体声 f32（含线性重采样）
    output.rs    cpal 输出回调：音量平滑、位置计数、样本 tap
  graphics.rs    Kitty / iTerm2 / 半块字符三种后端
  spectrum.rs    1024 点 FFT → 16 段对数频带
  term.rs        终端能力探测、raw mode 进出
  ui.rs          ratatui 布局与绘制
```

几处实现选择：

- 未使用 rodio：需要精确的 seek、位置与采样率适配，直接用 cpal + symphonia 更可控。
- 环形缓冲带背压：解码线程缓冲到约 3 秒即休眠，暂停时不会持续占用内存。
- 探测一律非阻塞：早期版本用阻塞 `read()` 等待终端回包，会吞掉用户的第一个按键，
  现改为 `O_NONBLOCK` + `poll`。
- 音频线程只做搬运：FFT 在 UI 线程计算，音频回调内只做一次 `try_lock` 拷贝。

## 开发用命令

```bash
ymp <目录> --selftest     # 无界面自检：扫描、逐首解码、播放/暂停/seek
ymp <目录> --render       # 离屏渲染界面为文本（调整布局时用）
```

## 分发

| 方式 | 命令 | 前提 |
| --- | --- | --- |
| 免编译包 | 解开 `ymp-<版本>-<arch>.tar.gz`，`cp ymp ~/.local/bin/`，再 `xattr -c` | 无 |
| crates.io | `cargo install ymp` | 已安装 Rust |
| 源码仓库 | `cargo install --git https://github.com/<仓库地址>` | 已安装 Rust |
| 自行构建 | `./install.sh` | 已安装 Rust |

打包：

```bash
./release.sh        # -> dist/ymp-<版本>-<arch>.tar.gz（含 README、LICENSE、INSTALL.txt）
```

脚本只构建当前架构。为 Intel Mac 打包需在该架构上执行，或
`rustup target add x86_64-apple-darwin` 后交叉编译。

> 从浏览器下载的二进制带 quarantine 属性，直接执行会被系统终止（`Killed: 9`，退出码 137），
> 因此 `xattr -c` 是必需步骤。`install.sh` 与包内 `INSTALL.txt` 均已包含该步骤。

## 已知限制

- 仅在 macOS（CoreAudio）上测试。cpal 本身跨平台，但未在 Linux / Windows 验证。
- 不支持 ALAC 编码的 m4a（Symphonia 限制）；同容器内的 AAC 正常。
- 播放队列为快照语义，不随搜索条件动态变化。
- 每次发送封面约 200-300KB（PNG + base64），本地终端无感知，SSH 远端会略慢。
- 宽度低于 40 列时自动隐藏封面，只显示列表。

## 许可

MIT，见 [LICENSE](LICENSE)。
