//! Bounded producer control-line I/O only. No caller, controller or authority.
use super::transport::Stream;
#[cfg(unix)]
use anyhow::ensure;
use anyhow::Result;
#[cfg(unix)]
use std::io::{BufRead, Read, Write};
use std::{
    io::{self, BufReader},
    time::{Duration, Instant},
};

pub(super) const LINE_BYTES: usize = 16 << 10;
pub(super) const MAX_WAIT: Duration = zerus_client_core::desktop::producer::timing::IDLE_LIMIT;

fn remaining(until: Instant) -> io::Result<Duration> {
    let left = until.saturating_duration_since(Instant::now());
    if left.is_zero() {
        return Err(io::ErrorKind::TimedOut.into());
    }
    if left > MAX_WAIT {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Ok(left)
}

#[cfg(unix)]
struct Until<'a> {
    reader: &'a mut BufReader<Stream>,
    until: Instant,
}
#[cfg(unix)]
impl Until<'_> {
    fn prepare(&self) -> io::Result<()> {
        self.reader
            .get_ref()
            .set_read_timeout(Some(remaining(self.until)?))
    }
}
#[cfg(unix)]
impl Read for Until<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.prepare()?;
        self.reader.read(bytes)
    }
}
#[cfg(unix)]
impl BufRead for Until<'_> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.prepare()?;
        self.reader.fill_buf()
    }
    fn consume(&mut self, count: usize) {
        self.reader.consume(count);
    }
}

/// Off-actor only. Reuse the existing reader, including its buffered tail.
/// One total deadline, at most 15 seconds, includes all partial progress. The
/// returned line includes LF; content/schema/identity still need validation.
pub(super) fn line(reader: &mut BufReader<Stream>, until: Instant) -> Result<Option<Vec<u8>>> {
    #[cfg(unix)]
    {
        remaining(until)?;
        let bytes = super::protocol::line(&mut Until { reader, until }, LINE_BYTES)?;
        remaining(until)?;
        Ok(bytes)
    }
    #[cfg(not(unix))]
    {
        let _ = (reader, until);
        anyhow::bail!("producer control I/O is unsupported")
    }
}

fn frame_valid(bytes: &[u8]) -> bool {
    (2..=LINE_BYTES).contains(&bytes.len())
        && bytes.last() == Some(&b'\n')
        && !bytes[..bytes.len() - 1].contains(&b'\n')
}

/// Writes one already encoded line, without cloning/closing the owned stream.
/// Ok means local write completion only, never peer acceptance or an effect
/// receipt. Any failed/late write may have sent bytes; it never permits replay.
pub(super) fn write_line(stream: &mut Stream, bytes: &[u8], until: Instant) -> Result<()> {
    #[cfg(unix)]
    {
        ensure!(frame_valid(bytes), "invalid producer control line");
        remaining(until)?;
        let mut written = 0;
        while written < bytes.len() {
            stream.set_write_timeout(Some(remaining(until)?))?;
            match stream.write(&bytes[written..]) {
                Ok(0) => return Err(io::Error::from(io::ErrorKind::WriteZero).into()),
                Ok(count) => written += count,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => (),
                Err(error) => return Err(error.into()),
            }
        }
        remaining(until)?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = (stream, bytes, until);
        anyhow::bail!("producer control I/O is unsupported")
    }
}

#[cfg(test)]
mod tests;
