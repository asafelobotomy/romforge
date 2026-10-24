//! Milestone 23: compressing a stream in fixed-size chunks on several
//! threads, written back in order. A few chunks per thread are read at a
//! time, so memory stays bounded whatever the file's size.

use std::io::{self, Read};

/// Read `src` in chunks of `chunk` bytes (the last one shorter), run
/// `work` on each (with its index) on up to `threads` threads, and give each result to
/// `put` in the stream's order with the plain chunk's length. A source
/// that ends early is fine: the caller checks the total.
pub fn in_order(
    src: &mut impl Read,
    chunk: usize,
    threads: usize,
    work: impl Fn(u64, &[u8]) -> io::Result<Vec<u8>> + Sync,
    mut put: impl FnMut(Vec<u8>, usize) -> io::Result<()>,
) -> io::Result<()> {
    let threads = threads.max(1);
    let mut next = 0u64;
    loop {
        let mut batch = Vec::with_capacity(threads * 2);
        for _ in 0..threads * 2 {
            let mut buf = vec![0u8; chunk];
            let n = read_full(src, &mut buf)?;
            if n == 0 {
                break;
            }
            buf.truncate(n);
            batch.push(buf);
            if n < chunk {
                break;
            }
        }
        if batch.is_empty() {
            return Ok(());
        }
        let last = batch.last().map_or(0, Vec::len) < chunk;
        let done: Vec<io::Result<Vec<u8>>> = if threads == 1 || batch.len() == 1 {
            batch.iter().zip(next..).map(|(b, i)| work(i, b)).collect()
        } else {
            std::thread::scope(|s| {
                let work = &work;
                let handles: Vec<_> = batch
                    .iter()
                    .zip(next..)
                    .map(|(b, i)| s.spawn(move || work(i, b)))
                    .collect();
                handles
                    .into_iter()
                    .map(|h| {
                        h.join()
                            .unwrap_or_else(|_| Err(io::Error::other("a compressing thread failed")))
                    })
                    .collect()
            })
        };
        next += batch.len() as u64;
        for (out, plain) in done.into_iter().zip(&batch) {
            put(out?, plain.len())?;
        }
        if last {
            return Ok(());
        }
    }
}

/// Fill `buf` as far as `r` goes; how many bytes were read.
pub fn read_full(r: &mut impl Read, buf: &mut [u8]) -> io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match r.read(&mut buf[n..])? {
            0 => break,
            k => n += k,
        }
    }
    Ok(n)
}

/// zstd at `level`, for one chunk.
pub fn zstd(data: &[u8], level: i32) -> io::Result<Vec<u8>> {
    zstd::bulk::compress(data, level)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_come_back_in_order() {
        let data: Vec<u8> = (0..1000u32).map(|i| (i % 7) as u8).collect();
        let mut got = Vec::new();
        in_order(
            &mut &data[..],
            64,
            4,
            |_, c| Ok(c.to_vec()),
            |out, n| {
                assert_eq!(out.len(), n);
                got.extend(out);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(got, data);
    }
}
