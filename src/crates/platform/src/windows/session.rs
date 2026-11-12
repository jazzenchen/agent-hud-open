//! Windows 結束工作階段時，阻止背景工作再建立外部程式。
use std::{
    io,
    process::{Child, Command},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, PoisonError,
    },
};

struct SpawnGate {
    ending: AtomicBool,
    launching: Mutex<()>,
}

impl SpawnGate {
    const fn new() -> Self {
        Self {
            ending: AtomicBool::new(false),
            launching: Mutex::new(()),
        }
    }

    fn set_ending(&self, ending: bool) {
        let _launching = self
            .launching
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        self.ending.store(ending, Ordering::Release);
    }

    fn run<T>(
        &self,
        system_ending: impl FnOnce() -> bool,
        launch: impl FnOnce() -> io::Result<T>,
    ) -> io::Result<T> {
        // 最後一次檢查和建立程序必須在同一把鎖內，不能只在配置命令時檢查。
        let _launching = self
            .launching
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if self.ending.load(Ordering::Acquire) || system_ending() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "Windows 工作階段正在結束，已略過背景程式啟動",
            ));
        }
        launch()
    }
}

static GATE: SpawnGate = SpawnGate::new();

/// 收到結束工作階段查詢時暫停；收到取消通知時恢復，不永久停用偵測。
pub fn set_ending(ending: bool) {
    GATE.set_ending(ending);
}

/// 系統狀態提供保底，也涵蓋未建立桌面視窗的命令列工作。
pub fn is_ending() -> bool {
    GATE.ending.load(Ordering::Acquire) || system_is_ending()
}

/// 已配置完成的命令，也必須在真正建立程序之前通過關機檢查。
pub fn spawn(command: &mut Command) -> io::Result<Child> {
    GATE.run(system_is_ending, || command.spawn())
}

fn system_is_ending() -> bool {
    // SAFETY：GetSystemMetrics 不接收指標，SM_SHUTTINGDOWN 只讀取工作階段狀態。
    unsafe { GetSystemMetrics(0x2000) != 0 }
}

#[link(name = "user32")]
extern "system" {
    fn GetSystemMetrics(index: i32) -> i32;
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
