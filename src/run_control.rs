use std::fmt;
use tokio_util::sync::CancellationToken;

#[derive(Default)]
pub struct RunControl {
    pub stop: CancellationToken,
    pub pause: CancellationToken,
}

#[derive(Debug)]
pub struct PauseRequested;

impl fmt::Display for PauseRequested {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("已暂停，可以手动操作手机；操作完成后点击继续")
    }
}
impl std::error::Error for PauseRequested {}
