use std::io::{Error, Read};

/// A BufferedReader wraps an io::Read and provides a buffer for reading.
/// It can be configured to sniff the data being read into a buffer.
pub struct BufferedReader<R: Read> {
    source: R,
    buffer: Vec<u8>,
    buffer_read: usize,
    buffer_size: usize,
    sniffing: bool,
}

impl<R: Read> BufferedReader<R> {
    pub fn new(source: R) -> Self {
        BufferedReader {
            source,
            buffer: Vec::new(),
            buffer_read: 0,
            buffer_size: 0,
            sniffing: false,
        }
    }

    /// Resets the BufferedReader to start sniffing or stop sniffing.
    pub fn reset(&mut self, sniffing: bool) {
        self.sniffing = sniffing;
        self.buffer_read = 0;
        self.buffer_size = self.buffer.len();
    }
}

impl<R: Read> Read for BufferedReader<R> {
    fn read(&mut self, p: &mut [u8]) -> Result<usize, Error> {
        if self.buffer_size > self.buffer_read {
            // There is still unread data in the buffer.
            let n = std::cmp::min(p.len(), self.buffer_size - self.buffer_read);
            p[..n].copy_from_slice(&self.buffer[self.buffer_read..self.buffer_read + n]);
            self.buffer_read += n;
            return Ok(n);
        } else if !self.sniffing && !self.buffer.is_empty() {
            // If not sniffing and there is still data in the buffer, clear it.
            self.buffer.clear();
        }

        // Read from the source.
        let sn = self.source.read(p)?;
        if sn > 0 && self.sniffing {
             // If sniffing is enabled, copy the read data to the buffer.
            self.buffer.extend_from_slice(&p[..sn]);
            self.buffer_size = self.buffer.len();
        }
        Ok(sn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_sniff_buffer_read() {
        let test_data = b"Hello, World!";
        let cursor = Cursor::new(test_data);
        let mut sniff_buffer = BufferedReader::new(cursor);

        // Test reading without sniffing
        let mut buf = [0u8; 5];
        assert_eq!(sniff_buffer.read(&mut buf).unwrap(), 5);
        assert_eq!(&buf, b"Hello");

        // Enable sniffing and read more
        sniff_buffer.reset(true);
        let mut buf = [0u8; 8];
        assert_eq!(sniff_buffer.read(&mut buf).unwrap(), 8);
        assert_eq!(&buf, b", World!");

        // Sniff done
        sniff_buffer.reset(false);

        // Read buffer data
        let mut buf = [0u8; 8];
        assert_eq!(sniff_buffer.read(&mut buf).unwrap(), 8);
        assert_eq!(sniff_buffer.buffer, b", World!");
    }
}
