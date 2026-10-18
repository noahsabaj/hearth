//! Standard base64, with padding: images in MCP tool results, and screenshots from the app.

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Encodes bytes as base64.
pub fn encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(chunk.get(1).copied().unwrap_or(0)) << 8)
            | u32::from(chunk.get(2).copied().unwrap_or(0));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Decodes base64, or `None` if the text isn't valid base64.
pub fn decode(text: &str) -> Option<Vec<u8>> {
    let mut values = [u8::MAX; 256];
    for (value, &symbol) in ALPHABET.iter().enumerate() {
        values[usize::from(symbol)] = value as u8;
    }
    let text = text.as_bytes();
    if !text.len().is_multiple_of(4) {
        return None;
    }
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    for (index, chunk) in text.chunks_exact(4).enumerate() {
        let last = index == text.len() / 4 - 1;
        let padding = if last {
            chunk
                .iter()
                .rev()
                .take_while(|&&symbol| symbol == b'=')
                .count()
        } else {
            0
        };
        if padding > 2 {
            return None;
        }
        let mut n = 0u32;
        for &symbol in &chunk[..4 - padding] {
            let value = values[usize::from(symbol)];
            if value == u8::MAX {
                return None;
            }
            n = n << 6 | u32::from(value);
        }
        n <<= 6 * padding as u32;
        out.extend_from_slice(&n.to_be_bytes()[1..4 - padding]);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes() {
        assert_eq!(encode(b""), "");
        assert_eq!(encode(b"f"), "Zg==");
        assert_eq!(encode(b"fo"), "Zm8=");
        assert_eq!(encode(b"foo"), "Zm9v");
        assert_eq!(encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn decodes() {
        for bytes in [&b""[..], b"f", b"fo", b"foo", b"foob", b"fooba", b"foobar"] {
            assert_eq!(decode(&encode(bytes)).as_deref(), Some(bytes));
        }
        let all: Vec<u8> = (0..=255).collect();
        assert_eq!(decode(&encode(&all)), Some(all));
        assert_eq!(decode("Zg="), None);
        assert_eq!(decode("Z==="), None);
        assert_eq!(decode("Zm9v!A=="), None);
        assert_eq!(decode("Zg==Zg=="), None);
    }
}
