use std::fs;
use std::path::Path;
use std::pin::Pin;

use super::DirEntry;
use crate::future::Future;
use crate::io;
use crate::task::{blocking, Context, Poll};

/// Returns a stream over the entries within a directory.
///
/// The stream yields items of type [`io::Result`]`<`[`DirEntry`]`>`. New errors may be encountered
/// after a stream is initially constructed.
///
/// This function is an async version of [`std::fs::read_dir`].
///
/// [`io::Result`]: https://doc.rust-lang.org/std/io/type.Result.html
/// [`DirEntry`]: struct.DirEntry.html
/// [`std::fs::read_dir`]: https://doc.rust-lang.org/std/fs/fn.read_dir.html
///
/// # Errors
///
/// An error will be returned in the following situations (not an exhaustive list):
///
/// * `path` does not exist.
/// * `path` does not point at a directory.
/// * The current process lacks permissions to view the contents of `path`.
///
/// # Examples
///
/// ```no_run
/// # fn main() -> std::io::Result<()> { async_std::task::block_on(async {
/// #
/// use async_std::fs;
/// use async_std::prelude::*;
///
/// let mut dir = fs::read_dir(".").await?;
///
/// while let Some(entry) = dir.next().await {
///     let entry = entry?;
///     println!("{:?}", entry.file_name());
/// }
/// #
/// # Ok(()) }) }
/// ```
pub async fn read_dir<P: AsRef<Path>>(path: P) -> io::Result<ReadDir> {
    let path = path.as_ref().to_owned();
    blocking::spawn(async move { fs::read_dir(path) })
        .await
        .map(ReadDir::new)
}

/// A stream over entries in a directory.
///
/// This stream is returned by [`read_dir`] and yields items of type
/// [`io::Result`]`<`[`DirEntry`]`>`. Each [`DirEntry`] can then retrieve information like entry's
/// path or metadata.
///
/// This type is an async version of [`std::fs::ReadDir`].
///
/// [`read_dir`]: fn.read_dir.html
/// [`io::Result`]: https://doc.rust-lang.org/std/io/type.Result.html
/// [`DirEntry`]: struct.DirEntry.html
/// [`std::fs::ReadDir`]: https://doc.rust-lang.org/std/fs/struct.ReadDir.html
#[derive(Debug)]
pub struct ReadDir(State);

/// The state of an asynchronous `ReadDir`.
///
/// The `ReadDir` can be either idle or busy performing an asynchronous operation.
#[derive(Debug)]
enum State {
    Idle(Option<fs::ReadDir>),
    Busy(blocking::JoinHandle<(fs::ReadDir, Option<io::Result<fs::DirEntry>>)>),
}

impl ReadDir {
    /// Creates an asynchronous `ReadDir` from a synchronous handle.
    pub(crate) fn new(inner: fs::ReadDir) -> ReadDir {
        ReadDir(State::Idle(Some(inner)))
    }
}

impl futures_core::stream::Stream for ReadDir {
    type Item = io::Result<DirEntry>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            match &mut self.0 {
                State::Idle(opt) => {
                    let mut inner = opt.take().unwrap();

                    // Start the operation asynchronously.
                    self.0 = State::Busy(blocking::spawn(async move {
                        let next = inner.next();
                        (inner, next)
                    }));
                }
                // Poll the asynchronous operation the file is currently blocked on.
                State::Busy(task) => {
                    let (inner, opt) = futures_core::ready!(Pin::new(task).poll(cx));
                    self.0 = State::Idle(Some(inner));
                    return Poll::Ready(opt.map(|res| res.map(DirEntry::new)));
                }
            }
        }
    }
}
