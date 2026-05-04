use anyhow::{Result, anyhow};
use futures_util::{Stream, stream};
use std::cmp::min;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeek, AsyncSeekExt, BufReader, SeekFrom};

static DEFAULT_SIZE: usize = 4096;

static LF_BYTE: u8 = b'\n';
static CR_BYTE: u8 = b'\r';

pub struct RevLine {
    pub line: String,
    pub position: u64,
}

/// `RevLines` struct
pub struct RevLines<R> {
    reader: BufReader<R>,
    reader_pos: u64,
    buf_size: u64,
}

impl<R: AsyncSeek + AsyncRead + Unpin> RevLines<R> {
    /// Create an async stream of strings from a `BufReader<R>`. Internal
    /// buffering for iteration will default to 4096 bytes at a time.
    pub async fn new_stream(reader: BufReader<R>, pos: Option<u64>) -> Result<impl Stream<Item = Result<RevLine>>> {
        RevLines::stream_with_capacity(DEFAULT_SIZE, pos, reader).await
    }

    /// Create an async stream of strings from a `BufReader<R>`. Internal
    /// buffering for iteration will use `cap` bytes at a time.
    pub async fn stream_with_capacity(
        cap: usize,
        pos: Option<u64>,
        mut reader: BufReader<R>,
    ) -> Result<impl Stream<Item = Result<RevLine>>> {
        // Seek to end of reader now
        let reader_size = reader
            .seek(match pos {
                Some(pos) => SeekFrom::Start(pos),
                None => SeekFrom::End(0),
            })
            .await?;

        let rev_lines = RevLines {
            reader,
            reader_pos: reader_size,
            buf_size: cap as u64,
        };

        let stream = stream::unfold(rev_lines, |mut rev_lines| async {
            rev_lines.next_line().await.map(|line| (line, rev_lines))
        });

        Ok(stream)
    }

    async fn read_to_buffer(&mut self, size: u64) -> Result<Vec<u8>, tokio::io::Error> {
        let mut buf = vec![0; size as usize];
        let offset = -(size as i64);

        self.reader.seek(SeekFrom::Current(offset)).await?;
        self.reader.read_exact(&mut buf[0..(size as usize)]).await?;
        self.reader.seek(SeekFrom::Current(offset)).await?;

        self.reader_pos -= size;

        Ok(buf)
    }

    async fn next_line(&mut self) -> Option<Result<RevLine>> {
        let mut result: Vec<u8> = Vec::new();

        'outer: loop {
            if self.reader_pos < 1 {
                if !result.is_empty() {
                    break;
                }

                return None;
            }

            // Read the of minimum between the desired
            // buffer size or remaining length of the reader
            let size = min(self.buf_size, self.reader_pos);

            match self.read_to_buffer(size).await {
                Ok(buf) => {
                    for (idx, ch) in (buf).iter().enumerate().rev() {
                        // Found a new line character to break on
                        if *ch == LF_BYTE {
                            let mut offset = idx as u64;

                            // Add an extra byte cause of CR character
                            if idx > 1 && buf[idx - 1] == CR_BYTE {
                                offset -= 1;
                            }

                            match self.reader.seek(SeekFrom::Current(offset as i64)).await {
                                Ok(_) => {
                                    self.reader_pos += offset;

                                    break 'outer;
                                }

                                Err(e) => return Some(Err(anyhow!("IO error: {e}"))),
                            }
                        } else {
                            result.push(*ch);
                        }
                    }
                }

                Err(e) => return Some(Err(anyhow!("IO error: {e}"))),
            }
        }

        // Reverse the results since they were written backwards
        result.reverse();

        // Convert to a String
        match String::from_utf8(result) {
            Ok(s) => Some(Ok(RevLine {
                line: s,
                position: self.reader_pos,
            })),
            Err(e) => Some(Err(anyhow!("from utf8 error: {e}"))),
        }
    }
}
