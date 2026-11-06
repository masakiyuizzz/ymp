//! Command line: one table of options, rendered in the chosen language.
//!
//! `-h` prints one line per option, `--help` prints the full text; `-v` prints
//! the version and `--version` the build details. Both pairs stay in sync
//! because they come from the same table, and clap's own help is switched off
//! so the layout, wording and language are ours.

use std::path::PathBuf;

use clap::{Parser, ValueEnum};

use crate::i18n::{self, t, Lang};

pub const NAME: &str = "ymp";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ModeArg {
    /// Sequential: first to last, then stop
    Seq,
    /// Reverse: last to first
    Rev,
    /// Shuffle the remaining tracks
    Shuffle,
    /// Repeat the current track
    One,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum GraphicsArg {
    /// Probe the terminal and pick the best backend
    Auto,
    /// Force the Kitty graphics protocol
    Kitty,
    /// Force the iTerm2 inline image protocol
    Iterm,
    /// Half-block characters, works anywhere
    Blocks,
    /// Do not draw cover art
    Off,
}

#[derive(Debug, Parser)]
#[command(name = "ymp", disable_help_flag = true, disable_version_flag = true)]
pub struct Args {
    #[arg(default_value = ".")]
    pub path: PathBuf,

    #[arg(short, long, value_enum, default_value = "seq")]
    pub mode: ModeArg,

    #[arg(long, default_value_t = 80, value_parser = clap::value_parser!(u8).range(0..=100))]
    pub volume: u8,

    #[arg(long)]
    pub play: bool,

    #[arg(long)]
    pub no_recursive: bool,

    #[arg(short, long, value_enum, default_value = "auto")]
    pub graphics: GraphicsArg,

    #[arg(long, value_name = "RATIO")]
    pub cell_aspect: Option<f32>,

    #[arg(long)]
    pub probe: bool,


    #[arg(long, hide = true)]
    pub selftest: bool,

    #[arg(long, hide = true)]
    pub render: bool,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            path: PathBuf::from("."),
            mode: ModeArg::Seq,
            volume: 80,
            play: false,
            no_recursive: false,
            graphics: GraphicsArg::Auto,
            cell_aspect: None,
            probe: false,
            selftest: false,
            render: false,
        }
    }
}

/// One option, described in both languages: the short form is a single line,
/// the long form is the paragraph `--help` prints.
struct Item {
    group: u8,
    flags: &'static str,
    /// The positional placeholder is the one flag that reads differently.
    flags_cn: Option<&'static str>,
    cn: (&'static str, &'static str),
    en: (&'static str, &'static str),
}

const GROUPS: [(&str, &str); 5] = [
    ("目标", "TARGET"),
    ("播放", "PLAYBACK"),
    ("画面", "DISPLAY"),
    ("诊断", "DIAGNOSTICS"),
    ("其它", "MISC"),
];

const ITEMS: &[Item] = &[
    Item {
        group: 0,
        flags: "[path]",
        flags_cn: Some("[路径]"),
        cn: (
            "音乐目录或音频文件（默认当前目录）",
            "目录或单个文件；目录默认递归子目录。\n默认值：. （当前目录）",
        ),
        en: (
            "folder or audio file (default: current directory)",
            "A folder or a single file; folders are scanned recursively.\nDefault: . (current directory)",
        ),
    },
    Item {
        group: 1,
        flags: "-m, --mode <MODE>",
        flags_cn: None,
        cn: (
            "播放顺序 seq / rev / shuffle / one（默认 seq）",
            "seq     从第一首往下放，放完停止\n\
             rev     从最后一首往上放\n\
             shuffle 把剩余曲目打乱后依次播放\n\
             one     单曲循环，一首放完重放\n\
             默认：seq",
        ),
        en: (
            "playback order: seq / rev / shuffle / one (default seq)",
            "seq     first to last, then stop\n\
             rev     last to first\n\
             shuffle shuffle the remaining tracks\n\
             one     repeat the current track\n\
             Default: seq",
        ),
    },
    Item {
        group: 1,
        flags: "    --volume <0-100>",
        flags_cn: None,
        cn: (
            "初始音量（默认 80）",
            "0-100 的整数，超出范围会报错。\n\
             运行时用 + / - 调整，立刻生效。\n\
             默认：80",
        ),
        en: (
            "starting volume (default 80)",
            "An integer from 0 to 100; anything else is rejected.\n\
             Adjust it while playing with + / -.\n\
             Default: 80",
        ),
    },
    Item {
        group: 1,
        flags: "    --play",
        flags_cn: None,
        cn: (
            "扫描完立即播放",
            "不加这个参数时只进界面，选中第一首等你按空格。",
        ),
        en: (
            "start playing right after scanning",
            "Without it ymp opens the UI and waits for space on the first track.",
        ),
    },
    Item {
        group: 1,
        flags: "    --no-recursive",
        flags_cn: None,
        cn: (
            "只看本层目录",
            "默认递归子目录；这个参数关掉递归，适合一张专辑一个文件夹的情况。",
        ),
        en: (
            "only this folder, do not descend",
            "Folders are recursive by default; this keeps it to one level, which is\n\
             what you want when every album lives in its own folder.",
        ),
    },
    Item {
        group: 2,
        flags: "-g, --graphics <BACKEND>",
        flags_cn: None,
        cn: (
            "封面方式 auto / kitty / iterm / blocks / off",
            "auto    先探测 Kitty 图形协议，再试 iTerm2 内联图，都不行就用半块字符\n\
             kitty   强制 Kitty 图形协议\n\
             iterm   强制 iTerm2 内联图片协议\n\
             blocks  只用半块字符画，任何终端都能看\n\
             off     完全不画封面\n\
             默认：auto",
        ),
        en: (
            "cover backend: auto / kitty / iterm / blocks / off",
            "auto    probe Kitty first, then iTerm2, else fall back to half blocks\n\
             kitty   force the Kitty graphics protocol\n\
             iterm   force the iTerm2 inline image protocol\n\
             blocks  half-block characters, works anywhere\n\
             off     no cover art\n\
             Default: auto",
        ),
    },
    Item {
        group: 2,
        flags: "    --cell-aspect <RATIO>",
        flags_cn: None,
        cn: (
            "单元格高宽比，探测失败时用",
            "探测拿不到真实单元格尺寸时，用它换算封面的行列数。\n\
             一般不用管；只在封面明显被拉长或压扁时再调。",
        ),
        en: (
            "cell height/width ratio when detection fails",
            "Used to convert the cover into rows and columns when the terminal does\n\
             not report its cell size. Only touch it if covers look stretched.",
        ),
    },
    Item {
        group: 3,
        flags: "    --probe",
        flags_cn: None,
        cn: (
            "打印终端能力后退出",
            "列出 TERM、图形协议支持、背景明暗、单元格像素尺寸。\n\
             排查「封面不显示」先跑这个。",
        ),
        en: (
            "print terminal capabilities and exit",
            "Shows TERM, graphics protocol support, light/dark background and the cell\n\
             size in pixels. First thing to run when covers do not show up.",
        ),
    },
    Item {
        group: 4,
        flags: "    --lang <LANG>",
        flags_cn: None,
                cn: (
            "界面语言；单独使用只写配置、不启动界面",
            "影响 --help、--version、界面提示和报错。\n\
             单独执行 --lang en 只写入配置并退出，不启动界面。\n\
             配置保存在 ~/.config/ymp/config（$XDG_CONFIG_HOME 优先）。\n\
             以下写法等价：--lang en、--lang=en、--language en。\n\
             值不合法会直接报错。\n\
             默认：cn",
        ),
        en: (
            "UI language; alone it only saves the preference",
            "Applies to --help, --version, the interface and error messages.\n\
             On its own, --lang en only saves the preference and exits.\n\
             The file is ~/.config/ymp/config (XDG_CONFIG_HOME wins).\n\
             Equivalent: --lang en, --lang=en, --language en.\n\
             An unknown value is an error.\n\
             Default: cn",
        ),
    },
    Item {
        group: 4,
        flags: "-h",
        flags_cn: None,
        cn: (
            "这份简洁说明",
            "每个功能一句话，适合查参数名。\n要看取值、默认值和示例用 --help。",
        ),
        en: (
            "this summary",
            "One line per option: good for remembering a flag name.\n\
             Values, defaults and examples live in --help.",
        ),
    },
    Item {
        group: 4,
        flags: "    --help",
        flags_cn: None,
        cn: ("完整说明：取值、默认值、示例、排查", "即本页内容。"),
        en: (
            "full text: values, defaults, examples",
            "What you are reading.",
        ),
    },
    Item {
        group: 4,
        flags: "-v, --version",
        flags_cn: None,
        cn: (
            "版本（--version 给完整信息）",
            "-v 打一行版本号（`-V` 同义）。\n\
             --version 打完整信息：版本、构建目标、解码与音频后端、终端要求、许可。",
        ),
        en: (
            "version (--version prints the full block)",
            "-v prints one line (`-V` works too).\n\
             --version prints version, build target, decoders, terminal requirements\n\
             and licence.",
        ),
    },
];

fn tagline() -> &'static str {
    t(
        "终端音乐播放器：读取本地音乐，封面以位图显示",
        "terminal music player with album art",
    )
}
/// Parse the command line, handling the flags that must work on their own.
/// Parse the command line.
///
/// The language flag is resolved first and then removed from the argument list,
/// so clap never sees it. Leaving it in is what let `--lang en` be read as
/// "`--lang` plus the positional `en`", which opened `./en` instead of switching
/// language.
pub fn parse() -> Args {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let (rest, wanted) = take_lang(&argv);

    // Explicit flag beats the saved preference, which beats Chinese.
    let lang = wanted
        .or_else(crate::config::read_lang)
        .unwrap_or(Lang::Cn);
    i18n::set(lang);

    // `-h` and `--version` use the language but must not change it.
    if let Some(code) = exits(&rest) {
        std::process::exit(code);
    }

    if let Some(l) = wanted {
        match crate::config::write_lang(l) {
            Ok(path) => {
                // Asking only for a language means "configure", not "start".
                if rest.is_empty() {
                    println!(
                        "{}",
                        i18n::p(
                            format!("语言已设为 {}，写入 {}", l.code(), path.display()),
                            format!("language set to {} in {}", l.code(), path.display()),
                        )
                    );
                    std::process::exit(0);
                }
            }
            Err(e) => {
                eprintln!(
                    "{}",
                    i18n::p(
                        format!("{NAME}: 无法写入配置文件：{e}"),
                        format!("{NAME}: cannot write the config file: {e}"),
                    )
                );
                if rest.is_empty() {
                    std::process::exit(1);
                }
            }
        }
    }
    match Args::try_parse_from(std::iter::once(NAME.to_string()).chain(rest)) {
        Ok(a) => a,
        Err(e) => {
            report(e);
            std::process::exit(2);
        }
    }
}

/// Pull `--lang` / `--language` out of the arguments and settle the language.
fn take_lang(argv: &[String]) -> (Vec<String>, Option<Lang>) {
    let mut rest = Vec::with_capacity(argv.len());
    let mut lang: Option<Lang> = None;
    let mut bad: Option<String> = None;
    let mut i = 0;
    while i < argv.len() {
        let a = &argv[i];
        if let Some(v) = a
            .strip_prefix("--lang=")
            .or_else(|| a.strip_prefix("--language="))
        {
            match Lang::parse(v) {
                Some(l) => lang = Some(l),
                None => bad = Some(v.to_string()),
            }
            i += 1;
        } else if a == "--lang" || a == "--language" {
            match argv.get(i + 1) {
                Some(next) if !next.starts_with('-') => {
                    match Lang::parse(next) {
                        Some(l) => lang = Some(l),
                        None => bad = Some(next.clone()),
                    }
                    i += 2;
                }
                // A bare `--lang` is a mistake worth naming, not a silent cn.
                _ => {
                    bad = Some(String::new());
                    i += 1;
                }
            }
        } else {
            rest.push(a.clone());
            i += 1;
        }
    }

    if let Some(v) = bad {
        // The language is exactly what could not be settled, so say it twice.
        if v.is_empty() {
            eprintln!("{NAME}: --lang 需要一个值，取 cn 或 en / --lang needs a value: cn or en");
        } else {
            eprintln!(
                "{NAME}: 无法识别的语言 \"{v}\"，取 cn 或 en / unknown language \"{v}\", use cn or en"
            );
        }
        std::process::exit(2);
    }
    (rest, lang)
}

/// `-h`, `--help`, `-v`, `--version` and `--lang` are handled before clap so
/// the language is settled first and help can be printed in it.
fn exits(argv: &[String]) -> Option<i32> {
    for (i, a) in argv.iter().enumerate() {
        // `-hv` / `-vh`: a cluster made only of help/version letters means help.
        let cluster = a.len() > 2
            && a.starts_with('-')
            && !a.starts_with("--")
            && a[1..].chars().all(|c| matches!(c, 'h' | 'v' | 'V'));
        match a.as_str() {
            "-h" => {
                short_help();
                return Some(0);
            }
            "--help" => {
                long_help();
                return Some(0);
            }
            "-v" | "-V" => {
                // `-v 80` used to mean volume; say why it printed a version
                // instead of silently swallowing the number.
                if let Some(next) = argv.get(i + 1) {
                    if !next.is_empty() && next.chars().all(|c| c.is_ascii_digit()) {
                        eprintln!(
                            "{}",
                            t(
                                "ymp 提示：音量已改为 --volume，-v 现在只打印版本号",
                                "ymp: volume is --volume now; -v only prints the version"
                            )
                        );
                    }
                }
                version_line();
                return Some(0);
            }
            "--version" => {
                long_version();
                return Some(0);
            }
            _ if cluster => {
                if a.contains('h') {
                    short_help();
                } else {
                    version_line();
                }
                return Some(0);
            }
            _ => {}
        }
    }
    None
}

fn flag_of(it: &Item, en: bool) -> &'static str {
    if en {
        it.flags
    } else {
        it.flags_cn.unwrap_or(it.flags)
    }
}

fn pad(s: &str, width: usize) -> String {
    let w = i18n::width(s);
    if w >= width {
        format!("{s} ")
    } else {
        format!("{s}{}", " ".repeat(width - w))
    }
}

fn version_line() {
    println!("{NAME} {VERSION}");
}

fn short_help() {
    let en = i18n::is_en();
    println!("{NAME} {VERSION} · {}", tagline());
    println!();
    println!("{}", t("用法", "Usage"));
    println!("  {NAME} {} [{}]", t("[选项]", "[options]"), t("路径", "path"));
    println!();
    println!("{}", t("选项", "Options"));
    for it in ITEMS {
        let line = if en { it.en.0 } else { it.cn.0 };
        println!("  {}{}", pad(flag_of(it, en), 28), line);
    }
    println!();
    println!(
        "{}",
        t(
            "取值、默认值和示例：ymp --help　　运行时按 ? 看快捷键",
            "values, defaults and examples: ymp --help    press ? for the key map"
        )
    );
}

fn long_help() {
    let en = i18n::is_en();
    println!("{NAME} {VERSION} · {}", tagline());
    println!();
    println!("{}", t("用法", "Usage"));
    println!("  {NAME} {} [{}]", t("[选项]", "[options]"), t("路径", "path"));
    println!();

    let mut group = u8::MAX;
    for it in ITEMS {
        if it.group != group {
            group = it.group;
            let (cn, en_t) = GROUPS[group as usize];
            println!("{}", if en { en_t } else { cn });
        }
        println!("  {}", flag_of(it, en));
        let text = if en { it.en.1 } else { it.cn.1 };
        for line in text.lines() {
            println!("      {line}");
        }
        println!();
    }

    println!("{}", t("示例", "Examples"));
    if en {
        println!("  ymp ~/Music/Album            play a whole folder");
        println!("  ymp song.flac                play one file");
        println!("  ymp ~/Music --play -m shuffle  scan, then shuffle-play");
        println!("  ymp --probe                  check terminal capabilities");
        println!("  ymp --lang en --help         this text in English");
    } else {
        println!("  ymp ~/Music/Album            播放整个文件夹");
        println!("  ymp song.flac                播放单个文件");
        println!("  ymp ~/Music --play -m shuffle  扫描后立刻随机播放");
        println!("  ymp --probe                  查看终端能力");
        println!("  ymp --lang en --help         看英文说明");
    }
    println!();
    println!("{}", t("退出码", "Exit codes"));
    if en {
        println!("  0 ok   1 runtime error   2 bad arguments");
    } else {
        println!("  0 正常   1 运行出错   2 参数有误");
    }
}

fn long_version() {
    let target = format!(
        "{}-{}",
        std::env::consts::ARCH,
        match std::env::consts::OS {
            "macos" => "apple-darwin",
            "linux" => "unknown-linux-gnu",
            other => other,
        }
    );
    println!("{NAME} {VERSION}");
    println!("{}", tagline());
    println!();
    if i18n::is_en() {
        println!("build target   {target}");
        println!("decoding       symphonia (flac / mp3 / m4a / wav / ogg / opus)");
        println!("audio output   cpal, system default device");
        println!("terminal       Kitty or iTerm2 graphics for cover art");
        println!("               half-block characters otherwise");
        println!("licence        MIT");
        println!("note           AI-generated, macOS only, no issue/PR support");
    } else {
        println!("构建目标    {target}");
        println!("解码        symphonia（flac / mp3 / m4a / wav / ogg / opus）");
        println!("音频输出    cpal，系统默认设备");
        println!("终端要求    Kitty 或 iTerm2 图形协议可画封面，否则退回半块字符");
        println!("许可        MIT");
        println!("说明        由 AI 生成，仅适配 macOS，不提供 issue / PR 支持");
    }
}

fn report(e: clap::Error) {
    use clap::error::{ContextKind, ContextValue, ErrorKind};

    let what = match e.kind() {
        ErrorKind::UnknownArgument => t("未知参数", "unknown argument"),
        ErrorKind::InvalidValue | ErrorKind::ValueValidation => t("参数值不合法", "invalid value"),
        ErrorKind::MissingRequiredArgument => t("缺少必需的参数", "missing argument"),
        ErrorKind::TooManyValues => t("给了太多值", "too many values"),
        ErrorKind::NoEquals => t("这里需要等号", "expected '='"),
        _ => t("参数有误", "bad arguments"),
    };
    let arg = match e.get(ContextKind::InvalidArg) {
        Some(ContextValue::String(s)) => format!(" `{s}`"),
        _ => String::new(),
    };
    let bad = match e.get(ContextKind::InvalidValue) {
        Some(ContextValue::String(s)) => format!(" = {s}"),
        _ => String::new(),
    };
    // Listing the accepted values is the difference between "it does not work"
    // and "oh, shuffle is spelt with one f".
    let mut valid: Vec<String> = Vec::new();
    for (kind, value) in e.context() {
        if kind == ContextKind::ValidValue {
            if let ContextValue::String(s) = value {
                valid.push(s.clone());
            }
        }
    }
    if valid.is_empty() {
        // clap renders the accepted values into the message instead of the
        // context; lift them out so they can be shown in either language.
        if let Some(rest) = e.to_string().split("possible values: ").nth(1) {
            let line = rest.lines().next().unwrap_or("");
            let line = line.trim().trim_start_matches('[').trim_end_matches(']');
            valid = line
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
    }

    eprintln!("{NAME}: {what}{arg}{bad}");
    if !valid.is_empty() {
        eprintln!("       {} {}", t("可选：", "valid:"), valid.join(" / "));
    }
    eprintln!(
        "       {}",
        t(
            "用 -h 看简洁用法，--help 看完整说明",
            "run `-h` for a summary, `--help` for the full text"
        )
    );
}
