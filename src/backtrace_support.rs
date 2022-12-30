use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::hash::{Hash, Hasher};

pub use backtrace;
use backtrace::{Backtrace, BacktraceFmt, BytesOrWideString, PrintFmt};

use crate::{BacktraceMode, Size};

#[derive(Clone)]
pub struct HashedBacktrace {
    inner: Option<Backtrace>,
    hash: u64,
}

pub struct TraceInfo {
    pub backtrace: HashedBacktrace,
    pub allocated: u64,
    pub freed: u64,
    pub mode: BacktraceMode,
}

struct HashedBacktraceShort<'a>(&'a HashedBacktrace);

impl<'a> fmt::Display for HashedBacktraceShort<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.display_short(f)
    }
}

impl HashedBacktrace {
    pub fn capture(mode: BacktraceMode) -> Self {
        if matches!(mode, BacktraceMode::None) {
            return Self {
                inner: None,
                hash: 0,
            };
        }
        let backtrace = Backtrace::new_unresolved();
        let mut hasher = DefaultHasher::new();
        backtrace
            .frames()
            .iter()
            .for_each(|x| hasher.write_u64(x.ip() as u64));
        let hash = hasher.finish();
        Self {
            inner: Some(backtrace),
            hash,
        }
    }

    pub fn inner(&self) -> &Backtrace {
        self.inner.as_ref().unwrap()
    }

    pub fn inner_mut(&mut self) -> &mut Backtrace {
        self.inner.as_mut().unwrap()
    }

    pub fn hash(&self) -> u64 {
        self.hash
    }

    fn display_short(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let full = f.alternate();
        let frames = self.inner().frames();

        let cwd = std::env::current_dir();
        let mut print_path = move |fmt: &mut fmt::Formatter<'_>, path: BytesOrWideString<'_>| {
            let path = path.into_path_buf();
            if !full {
                if let Ok(cwd) = &cwd {
                    if let Ok(suffix) = path.strip_prefix(cwd) {
                        return fmt::Display::fmt(&suffix.display(), fmt);
                    }
                }
            }
            fmt::Display::fmt(&path.display(), fmt)
        };

        let mut f = BacktraceFmt::new(f, PrintFmt::Short, &mut print_path);
        f.add_context()?;
        for frame in frames {
            let symbols = frame.symbols();
            for symbol in symbols {
                if let Some(name) = symbol.name().map(|x| x.to_string()) {
                    let name = name.strip_prefix('<').unwrap_or(&name);
                    if name.starts_with("alloc_track::")
                        || name == "__rg_alloc"
                        || name.starts_with("alloc::")
                        || name.starts_with("std::panicking::")
                        || name == "__rust_try"
                        || name == "_start"
                        || name == "__libc_start_main_impl"
                        || name == "__libc_start_call_main"
                        || name.starts_with("std::rt::")
                    {
                        continue;
                    }
                }

                f.frame().backtrace_symbol(frame, symbol)?;
            }
            if symbols.is_empty() {
                f.frame().print_raw(frame.ip(), None, None, None)?;
            }
        }
        f.finish()?;
        Ok(())
    }
}

impl PartialEq for HashedBacktrace {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash
    }
}

impl Eq for HashedBacktrace {}

impl Hash for HashedBacktrace {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.hash.hash(state);
    }
}

#[derive(Debug, Clone, Default)]
pub struct BacktraceMetric {
    pub allocated: u64,
    pub freed: u64,
    pub mode: BacktraceMode,
}

impl fmt::Display for BacktraceMetric {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "allocated: {}", Size(self.allocated))?;
        writeln!(f, "freed: {}", Size(self.freed))?;
        writeln!(
            f,
            "total_used: {}",
            Size(self.allocated.saturating_sub(self.freed))
        )?;
        Ok(())
    }
}

pub struct BacktraceReport(pub Vec<(HashedBacktrace, BacktraceMetric)>);

impl fmt::Display for BacktraceReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (backtrace, metric) in &self.0 {
            match metric.mode {
                BacktraceMode::None => unreachable!(),
                BacktraceMode::Short => {
                    writeln!(f, "{}\n{metric}\n\n", HashedBacktraceShort(backtrace))?
                }
                BacktraceMode::Full => writeln!(f, "{:?}\n{metric}\n\n", backtrace.inner())?,
            }
        }
        Ok(())
    }
}
