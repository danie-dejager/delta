// https://github.com/sharkdp/bat a1b9334a44a2c652f52dddaa83dbacba57372468
// src/output.rs
// See src/utils/bat/LICENSE
use std::ffi::OsString;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

use super::less::retrieve_less_version;

use crate::config;
use crate::env::DeltaEnv;
use crate::fatal;
use crate::features::navigate;

#[derive(Debug, Default)]
pub struct PagerCfg {
    pub navigate: bool,
    pub show_themes: bool,
    pub navigate_regex: Option<String>,
}

impl From<&config::Config> for PagerCfg {
    fn from(cfg: &config::Config) -> Self {
        PagerCfg {
            navigate: cfg.navigate,
            show_themes: cfg.show_themes,
            navigate_regex: cfg.navigate_regex.clone(),
        }
    }
}
impl From<config::Config> for PagerCfg {
    fn from(cfg: config::Config) -> Self {
        (&cfg).into()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)]
pub enum PagingMode {
    Always,
    QuitIfOneScreen,
    #[default]
    Never,
    Capture,
}
const LESSUTFCHARDEF: &str = "LESSUTFCHARDEF";
use crate::errors::*;

pub enum OutputType {
    Pager(Child),
    Stdout(io::Stdout),
    Capture,
}

impl Drop for OutputType {
    fn drop(&mut self) {
        if let OutputType::Pager(ref mut command) = *self {
            let _ = command.wait();
        }
    }
}

impl OutputType {
    /// Create a pager and write all data into it. Waits until the pager exits.
    /// The expectation is that the program will exit afterwards.
    pub fn oneshot_write(data: String) -> io::Result<()> {
        let mut output_type = OutputType::from_mode(
            &DeltaEnv::init(),
            PagingMode::QuitIfOneScreen,
            None,
            &PagerCfg::default(),
        )
        .unwrap();
        let mut writer = output_type.handle().unwrap();
        write!(&mut writer, "{data}")
    }

    pub fn from_mode(
        env: &DeltaEnv,
        mode: PagingMode,
        pager: Option<String>,
        config: &PagerCfg,
    ) -> Result<Self> {
        use self::PagingMode::*;
        Ok(match mode {
            Always => OutputType::try_pager(env, false, pager, config)?,
            QuitIfOneScreen => OutputType::try_pager(env, true, pager, config)?,
            Capture => OutputType::Capture,
            _ => OutputType::stdout(),
        })
    }

    /// Try to launch the pager. Fall back to stdout in case of errors.
    fn try_pager(
        env: &DeltaEnv,
        quit_if_one_screen: bool,
        pager_from_config: Option<String>,
        config: &PagerCfg,
    ) -> Result<Self> {
        let mut replace_arguments_to_less = false;

        let pager_from_env = match env.pagers.clone() {
            (Some(delta_pager), _) => Some(delta_pager),
            (_, Some(pager)) => {
                // less needs to be called with the '-R' option in order to properly interpret ANSI
                // color sequences. If someone has set PAGER="less -F", we therefore need to
                // overwrite the arguments and add '-R'.
                // We only do this for PAGER, since it is used in other contexts.
                replace_arguments_to_less = true;
                Some(pager)
            }
            _ => None,
        };

        if pager_from_config.is_some() {
            replace_arguments_to_less = false;
        }

        let pager_cmd = shell_words::split(
            &pager_from_config
                .or(pager_from_env)
                .unwrap_or_else(|| String::from("less")),
        )
        .context("Could not parse pager command.")?;

        Ok(match pager_cmd.split_first() {
            Some((pager_path, args)) => {
                let pager_path = PathBuf::from(pager_path);

                let is_less = pager_path.file_stem() == Some(&OsString::from("less"));

                let process = if is_less {
                    _make_process_from_less_path(
                        pager_path,
                        args,
                        replace_arguments_to_less,
                        quit_if_one_screen,
                        config,
                    )
                } else {
                    _make_process_from_pager_path(pager_path, args)
                };
                if let Some(mut process) = process {
                    process
                        .stdin(Stdio::piped())
                        .spawn()
                        .map(OutputType::Pager)
                        .unwrap_or_else(|_| OutputType::stdout())
                } else {
                    OutputType::stdout()
                }
            }
            None => OutputType::stdout(),
        })
    }

    fn stdout() -> Self {
        OutputType::Stdout(io::stdout())
    }

    pub fn handle(&mut self) -> Result<&mut dyn Write> {
        Ok(match *self {
            OutputType::Pager(ref mut command) => command
                .stdin
                .as_mut()
                .context("Could not open stdin for pager")?,
            OutputType::Stdout(ref mut handle) => handle,
            OutputType::Capture => unreachable!("capture can not be set"),
        })
    }
}

fn _make_process_from_less_path(
    less_path: PathBuf,
    args: &[String],
    replace_arguments_to_less: bool,
    quit_if_one_screen: bool,
    config: &PagerCfg,
) -> Option<Command> {
    if let Ok(less_path) = grep_cli::resolve_binary(less_path) {
        let mut p = Command::new(less_path.clone());
        if args.is_empty() || replace_arguments_to_less {
            p.args(vec!["--RAW-CONTROL-CHARS"]);

            // Passing '--no-init' fixes a bug with '--quit-if-one-screen' in older
            // versions of 'less'. Unfortunately, it also breaks mouse-wheel support.
            //
            // See: http://www.greenwoodsoftware.com/less/news.530.html
            //
            // For newer versions (530 or 558 on Windows), we omit '--no-init' as it
            // is not needed anymore.
            match retrieve_less_version(less_path) {
                None => {
                    p.arg("--no-init");
                }
                Some(version) if (version < 530 || (cfg!(windows) && version < 558)) => {
                    p.arg("--no-init");
                }
                _ => {}
            }

            if quit_if_one_screen {
                p.arg("--quit-if-one-screen");
            }
        } else {
            p.args(args);
        }

        // less >= 633 (from May 2023) prints any characters from the Private Use Area of Unicode
        // as control characters (e.g. <U+E012> instead of hoping that the terminal can render it).
        // This means any Nerd Fonts will not be displayed properly. Previous versions of less just
        // passed these characters through, and terminals usually fall back to a less obtrusive
        // box. Use this new env var less introduced to restore the previous behavior. This sets all
        // chars to single width (':p', see less manual). If a user provided env var is present,
        // use do not override it.
        // Also see delta issue 1616 and nerd-fonts/issues/1337
        if std::env::var(LESSUTFCHARDEF).is_err() {
            p.env(LESSUTFCHARDEF, "E000-F8FF:p,F0000-FFFFD:p,100000-10FFFD:p");
        }

        p.env("LESSCHARSET", "UTF-8");
        p.env("LESSANSIENDCHARS", "mK");

        if config.navigate {
            if let Ok(hist_file) = navigate::copy_less_hist_file_and_append_navigate_regex(config) {
                p.env("LESSHISTFILE", hist_file);
                if config.show_themes {
                    p.arg("+n");
                }
            }
        }
        Some(p)
    } else {
        None
    }
}

fn _make_process_from_pager_path(pager_path: PathBuf, args: &[String]) -> Option<Command> {
    if pager_path.file_stem() == Some(&OsString::from("delta")) {
        fatal(
            "\
It looks like you have set delta as the value of $PAGER. \
This would result in a non-terminating recursion. \
delta is not an appropriate value for $PAGER \
(but it is an appropriate value for $GIT_PAGER).",
        );
    }
    if let Ok(pager_path) = grep_cli::resolve_binary(pager_path) {
        let mut p = Command::new(pager_path);
        p.args(args);
        Some(p)
    } else {
        None
    }
}
