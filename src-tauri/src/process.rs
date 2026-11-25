use std::{
    fs::{self, DirBuilder},
    io::{self, Read, Write},
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::{Condvar, Mutex},
    time::{Duration, Instant},
};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Default, PartialEq)]
enum Phase {
    #[default]
    Running,
    Waiting,
    Cancelled,
    Saved,
}

#[derive(Default)]
pub struct Control {
    phase: Mutex<Phase>,
    changed: Condvar,
}

impl Control {
    pub fn cancelled(&self) -> bool {
        *self.phase.lock().unwrap() == Phase::Cancelled
    }

    pub fn check(&self) -> Result<()> {
        if self.cancelled() {
            return Err("测速已取消".into());
        }
        Ok(())
    }

    pub fn sleep(&self, time: Duration) -> Result<()> {
        let (phase, _) = self
            .changed
            .wait_timeout_while(self.phase.lock().unwrap(), time, |phase| {
                *phase != Phase::Cancelled
            })
            .unwrap();
        if *phase == Phase::Cancelled {
            return Err("测速已取消".into());
        }
        Ok(())
    }

    pub fn confirm(&self, notify: impl FnOnce()) -> Result<()> {
        let mut phase = self.phase.lock().unwrap();
        if *phase == Phase::Cancelled {
            return Err("测速已取消".into());
        }
        *phase = Phase::Waiting;
        drop(phase);
        notify();
        drop(
            self.changed
                .wait_while(self.phase.lock().unwrap(), |p| *p == Phase::Waiting)
                .unwrap(),
        );
        self.check()
    }

    pub fn approve(&self) -> bool {
        let mut phase = self.phase.lock().unwrap();
        if *phase != Phase::Waiting {
            return false;
        }
        *phase = Phase::Running;
        self.changed.notify_all();
        true
    }

    pub fn cancel(&self) {
        let mut phase = self.phase.lock().unwrap();
        if *phase != Phase::Saved {
            *phase = Phase::Cancelled;
            self.changed.notify_all();
        }
    }

    pub fn commit(&self, finish: bool, commit: impl FnOnce() -> io::Result<()>) -> Result<()> {
        let mut phase = self.phase.lock().unwrap();
        if *phase == Phase::Cancelled {
            return Err("测速已取消".into());
        }
        commit()?;
        if finish {
            *phase = Phase::Saved;
        }
        Ok(())
    }
}

pub fn command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut command = Command::new(program);
    command
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

pub struct Running(Child);

impl Running {
    pub fn start(command: &mut Command, control: &Control) -> io::Result<Self> {
        if control.cancelled() {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "测速已取消"));
        }
        command.spawn().map(Self)
    }

    pub fn exited(&mut self) -> io::Result<bool> {
        self.0.try_wait().map(|status| status.is_some())
    }

    pub fn wait(&mut self, control: &Control, timeout: Option<Duration>) -> Result<ExitStatus> {
        let start = Instant::now();
        loop {
            control.check()?;
            if let Some(status) = self.0.try_wait()? {
                return Ok(status);
            }
            if timeout.is_some_and(|limit| start.elapsed() >= limit) {
                return Err(io::Error::new(io::ErrorKind::TimedOut, "子进程超时").into());
            }
            control.sleep(Duration::from_millis(if timeout.is_some() {
                5
            } else {
                35
            }))?;
        }
    }

    pub fn copy_stdout(&mut self, destination: &mut impl io::Write, limit: u64) -> Result<u64> {
        let stdout = self.0.stdout.take().ok_or("子进程没有标准输出")?;
        let bytes = io::copy(&mut stdout.take(limit + 1), destination)?;
        if bytes > limit {
            return Err("组件下载或解压大小超出限制".into());
        }
        Ok(bytes)
    }

    pub fn exchange(
        mut self,
        input: &[u8],
        limit: u64,
        control: &Control,
        timeout: Duration,
    ) -> Result<(ExitStatus, Vec<u8>)> {
        self.0
            .stdin
            .take()
            .ok_or("子进程没有标准输入")?
            .write_all(input)?;
        self.capture(limit, control, timeout)
    }

    pub fn capture(
        mut self,
        limit: u64,
        control: &Control,
        timeout: Duration,
    ) -> Result<(ExitStatus, Vec<u8>)> {
        let stdout = self.0.stdout.take().ok_or("子进程没有标准输出")?;
        // Drain the pipe without blocking cancellation of the child.
        let reader = std::thread::spawn(move || {
            let mut bytes = Vec::new();
            stdout.take(limit + 1).read_to_end(&mut bytes)?;
            Ok::<_, io::Error>(bytes)
        });
        let status = self.wait(control, Some(timeout));
        drop(self);
        let bytes = reader.join().map_err(|_| "无法读取子进程输出")??;
        if bytes.len() as u64 > limit {
            return Err("子进程输出超过大小限制".into());
        }
        Ok((status?, bytes))
    }

    // Only used for short command output.
    pub fn output(&mut self, control: &Control, timeout: Duration) -> Result<(ExitStatus, String)> {
        let status = self.wait(control, Some(timeout))?;
        let mut output = String::new();
        if let Some(mut stdout) = self.0.stdout.take() {
            stdout.read_to_string(&mut output)?;
        }
        Ok((status, output))
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

pub struct PrivateDir(pub PathBuf);

impl PrivateDir {
    pub fn new(path: PathBuf) -> io::Result<Self> {
        DirBuilder::new().mode(0o700).create(&path)?;
        Ok(Self(path))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for PrivateDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_can_stop_after_the_plan_but_not_after_completion() {
        let control = Control::default();
        control.commit(false, || Ok(())).unwrap();
        control.cancel();
        assert!(control.cancelled());
        assert!(
            control
                .commit(false, || panic!("cancelled plan was saved"))
                .is_err()
        );

        let control = Control::default();
        control.commit(true, || Ok(())).unwrap();
        control.cancel();
        assert!(!control.cancelled());
    }

    #[test]
    fn approval_and_cancellation_wake_the_run() {
        let control = Control::default();
        assert!(!control.approve());
        control.confirm(|| assert!(control.approve())).unwrap();
        let mut child = Running::start(command("/bin/sleep").arg("10"), &control).unwrap();
        control.cancel();
        assert!(child.wait(&control, None).is_err());
        assert!(
            control
                .commit(true, || panic!("cancelled run published"))
                .is_err()
        );
        let pid = child.0.id();
        drop(child);
        assert!(
            !command("/bin/ps")
                .args(["-p", &pid.to_string()])
                .status()
                .unwrap()
                .success()
        );
    }
}
