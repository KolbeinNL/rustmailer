// Copyright © 2025-2026 rustmailer.com
// Licensed under RustMailer License Agreement v1.0
// Unauthorized copying, modification, or distribution is prohibited.

use std::pin::Pin;
use std::task::{Context, Poll};
// use std::time::Duration;

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use crate::modules::imap::session::SessionStream;
use crate::modules::metrics::{RECEIVED, RUSTMAILER_IMAP_TRAFFIC_TOTAL_BY_METRIC, SENT};

pub struct StatsWrapper<T> {
    inner: T,
}

impl<T> StatsWrapper<T> {
    pub fn new(inner: T) -> Self {
        Self { inner }
    }
}

impl<T: AsyncRead + Unpin> AsyncRead for StatsWrapper<T> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        let result = Pin::new(&mut self.inner).poll_read(cx, buf);
        if let Poll::Ready(Ok(())) = &result {
            let bytes_read = buf.filled().len() - before;
            RUSTMAILER_IMAP_TRAFFIC_TOTAL_BY_METRIC
                .with_label_values(&[RECEIVED])
                .inc_by(bytes_read as u64);
        }
        result
    }
}

impl<T: AsyncWrite + Unpin> AsyncWrite for StatsWrapper<T> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let result = Pin::new(&mut self.inner).poll_write(cx, buf);
        if let Poll::Ready(Ok(bytes_written)) = &result {
            RUSTMAILER_IMAP_TRAFFIC_TOTAL_BY_METRIC
                .with_label_values(&[SENT])
                .inc_by(*bytes_written as u64);
        }
        result
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }

    fn is_write_vectored(&self) -> bool {
        false
    }
}

impl<T: SessionStream> SessionStream for StatsWrapper<T> {
    // fn set_read_timeout(&mut self, timeout: Option<Duration>) {
    //     self.inner.set_read_timeout(timeout);
    // }
}

impl<T: SessionStream> std::fmt::Debug for StatsWrapper<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StatsWrapper")
            .field("inner", &self.inner)
            .finish()
    }
}
