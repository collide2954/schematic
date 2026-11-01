use std::path::Path;

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01,
    0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc,
    0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147,
    0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08,
    0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
    0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

pub struct Sha256 {
    state: [u32; 8],
    buffer: Vec<u8>,
    length: u64,
}

impl Default for Sha256 {
    fn default() -> Sha256 {
        Sha256::new()
    }
}

impl Sha256 {
    pub fn new() -> Sha256 {
        Sha256 {
            state: [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19],
            buffer: Vec::with_capacity(64),
            length: 0,
        }
    }

    pub fn update(&mut self, data: &[u8]) {
        self.length += data.len() as u64;
        self.buffer.extend_from_slice(data);
        let blocks = self.buffer.len() / 64;
        for i in 0..blocks {
            let block: [u8; 64] = self.buffer[i * 64..(i + 1) * 64].try_into().expect("a full block");
            self.compress(&block);
        }
        self.buffer.drain(..blocks * 64);
    }

    fn compress(&mut self, block: &[u8; 64]) {
        let mut w = [0u32; 64];
        for (i, chunk) in block.as_chunks::<4>().0.iter().enumerate() {
            w[i] = u32::from_be_bytes(*chunk);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choice = (e & f) ^ (!e & g);
            let t1 = h.wrapping_add(s1).wrapping_add(choice).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(majority);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (slot, value) in self.state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *slot = slot.wrapping_add(value);
        }
    }

    pub fn finish(mut self) -> [u8; 32] {
        let bits = self.length.wrapping_mul(8);
        let mut padding = vec![0x80u8];
        while (self.buffer.len() + padding.len()) % 64 != 56 {
            padding.push(0);
        }
        padding.extend_from_slice(&bits.to_be_bytes());
        self.update(&padding);
        let mut out = [0u8; 32];
        for (chunk, word) in out.as_chunks_mut::<4>().0.iter_mut().zip(self.state) {
            *chunk = word.to_be_bytes();
        }
        out
    }

    pub fn finish_hex(self) -> String {
        hex(&self.finish())
    }
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finish_hex()
}

fn files_under(dir: &Path, prefix: &Path, out: &mut Vec<(String, std::path::PathBuf)>) -> std::io::Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name();
        if name == ".git" {
            continue;
        }
        let relative = prefix.join(&name);
        if entry.file_type()?.is_dir() {
            files_under(&entry.path(), &relative, out)?;
        } else {
            out.push((relative.to_string_lossy().replace('\\', "/"), entry.path()));
        }
    }
    Ok(())
}

pub fn hash_tree(dir: &Path) -> std::io::Result<String> {
    let mut files = Vec::new();
    files_under(dir, Path::new(""), &mut files)?;
    files.sort();
    let mut hasher = Sha256::new();
    for (relative, path) in files {
        let content = std::fs::read(&path)?;
        hasher.update(relative.as_bytes());
        hasher.update(&[0]);
        hasher.update(&(content.len() as u64).to_be_bytes());
        hasher.update(&content);
    }
    Ok(format!("sha256:{}", hasher.finish_hex()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_the_known_vectors() {
        assert_eq!(sha256_hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(
            sha256_hex(b"The quick brown fox jumps over the lazy dog"),
            "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592"
        );
        let mut streamed = Sha256::new();
        for _ in 0..1000 {
            streamed.update(&[b'a'; 1000]);
        }
        assert_eq!(streamed.finish_hex(), "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0");
        let mut split = Sha256::new();
        split.update(b"ab");
        split.update(b"c");
        assert_eq!(split.finish_hex(), sha256_hex(b"abc"));
    }

    #[test]
    fn tree_hashes_follow_contents_and_ignore_git_folders() {
        let dir = std::env::temp_dir().join(format!("schematic-hash-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        std::fs::write(dir.join("project.scm"), "{:name \"x\"}").unwrap();
        std::fs::write(dir.join("src").join("x.scm"), "(ns x)").unwrap();
        std::fs::write(dir.join(".git").join("HEAD"), "ref").unwrap();
        let first = hash_tree(&dir).unwrap();
        assert!(first.starts_with("sha256:") && first.len() == 71);
        std::fs::write(dir.join(".git").join("HEAD"), "other").unwrap();
        assert_eq!(hash_tree(&dir).unwrap(), first);
        std::fs::write(dir.join("src").join("x.scm"), "(ns x) 1").unwrap();
        assert_ne!(hash_tree(&dir).unwrap(), first);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
