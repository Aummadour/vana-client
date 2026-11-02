//! polcore's stream cipher (both services): Blowfish-16 over tables expanded from the key by MD5 instead of pi, run
//! as an output-feedback keystream that lets CR and LF through. Addresses polcore 73b1864b;
//! MODLOG "PlayOnline sign-in".

/// MD5 of `data` from `state` with the length counted from zero: polcore's key expansion finalises one context and
/// keeps hashing into it, so every round after the first starts from an all-zero state (0x10018910 zeroes it).
fn md5_from(state: [u32; 4], data: &[u8]) -> [u8; 16] {
    let mut st = state;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&((data.len() as u64) * 8).to_le_bytes());
    let k: [u32; 64] = std::array::from_fn(|i| ((i as f64 + 1.0).sin().abs() * 4_294_967_296.0) as u32);
    for block in msg.chunks_exact(64) {
        md5_block(&mut st, &k, block);
    }
    let mut out = [0u8; 16];
    for (i, w) in st.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&w.to_le_bytes());
    }
    out
}

/// RFC 1321's compression of one block.
fn md5_block(st: &mut [u32; 4], k: &[u32; 64], block: &[u8]) {
    const S: [u32; 16] = [7, 12, 17, 22, 5, 9, 14, 20, 4, 11, 16, 23, 6, 10, 15, 21];
    let x: [u32; 16] = std::array::from_fn(|i| u32::from_le_bytes(block[i * 4..i * 4 + 4].try_into().unwrap()));
    let [mut a, mut b, mut c, mut d] = *st;
    for i in 0..64 {
        let (f, g) = match i / 16 {
            0 => ((b & c) | (!b & d), i),
            1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
            2 => (b ^ c ^ d, (3 * i + 5) % 16),
            _ => (c ^ (b | !d), (7 * i) % 16),
        };
        let t = a.wrapping_add(f).wrapping_add(k[i]).wrapping_add(x[g]).rotate_left(S[i / 16 * 4 + i % 4]);
        (a, d, c) = (d, c, b);
        b = b.wrapping_add(t);
    }
    for (s, v) in st.iter_mut().zip([a, b, c, d]) {
        *s = s.wrapping_add(v);
    }
}

#[derive(Clone)]
pub struct PolBlowfish {
    p: [u32; 18],
    s: Box<[u32; 1024]>,
    iv: (u32, u32),
    run: (u32, u32),
    pos: u32,
}

impl PolBlowfish {
    /// The schedule (0x10064300) and the context (0x10063eb0): the IV is the client modulus's first 8 bytes.
    pub fn new(key: &[u8], iv: [u8; 8]) -> Self {
        // Expand to 0x1000 bytes: each round hashes everything so far, the first from MD5's IV, later ones from zero.
        let mut buf = key.to_vec();
        let mut state = [0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476];
        while buf.len() < 0x1000 {
            let d = md5_from(state, &buf);
            state = [0; 4];
            let take = 16.min(0x1000 - buf.len());
            buf.extend_from_slice(&d[..take]);
        }
        let be = |i: usize| u32::from_be_bytes(buf[i..i + 4].try_into().unwrap());
        // P from overlapping windows (stride 1), S from consecutive words, P then xored with the buffer's words.
        let iv = (u32::from_le_bytes(iv[..4].try_into().unwrap()), u32::from_le_bytes(iv[4..].try_into().unwrap()));
        let mut bf = Self { p: std::array::from_fn(|i| be(i) ^ be(4 * i)), s: Box::new(std::array::from_fn(|j| be(4 * j))), iv, run: iv, pos: 0 };
        // Each pair from the zero block with the tables as they stand: no chaining.
        for i in (0..18).step_by(2) {
            (bf.p[i], bf.p[i + 1]) = bf.block(0, 0);
        }
        for j in (0..1024).step_by(2) {
            (bf.s[j], bf.s[j + 1]) = bf.block(0, 0);
        }
        bf
    }

    fn f(&self, x: u32) -> u32 {
        let s = &self.s;
        (s[(x >> 24) as usize].wrapping_add(s[256 + (x >> 16 & 255) as usize]) ^ s[512 + (x >> 8 & 255) as usize]).wrapping_add(s[768 + (x & 255) as usize])
    }

    /// 0x10064220: Blowfish-16 with xL = lo, xR = hi.
    pub fn block(&self, lo: u32, hi: u32) -> (u32, u32) {
        let (mut l, mut r) = (lo, hi);
        for i in 0..16 {
            l ^= self.p[i];
            r ^= self.f(l);
            std::mem::swap(&mut l, &mut r);
        }
        std::mem::swap(&mut l, &mut r);
        (l ^ self.p[17], r ^ self.p[16])
    }

    /// 0x10063ef0: a keystream block every 8 bytes; a byte that is CR or LF before or after the xor goes as it was,
    /// its keystream byte used anyway. `reset` restarts from the IV.
    pub fn stream(&mut self, data: &mut [u8], reset: bool) {
        let ((mut lo, mut hi), mut pos) = if reset { (self.iv, 0) } else { (self.run, self.pos) };
        for b in data {
            if pos & 7 == 0 {
                (lo, hi) = self.block(lo, hi);
            }
            let k = ((hi as u64) << 32 | lo as u64) >> (8 * (pos & 7)) & 0xff;
            pos = (pos & 7) + 1;
            let c = *b ^ k as u8;
            if !matches!(*b, b'\n' | b'\r') && !matches!(c, b'\n' | b'\r') {
                *b = c;
            }
        }
        self.run = (lo, hi);
        self.pos = pos;
    }
}

impl super::polpro::Stream for PolBlowfish {
    fn apply(&mut self, data: &mut [u8], reset: bool) {
        self.stream(data, reset);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(s: &str) -> Vec<u8> {
        hex::decode(s).unwrap()
    }

    // Vectors from polcore 73b1864b run under Wine (lab: scratch/pol/chat/vectors.json).

    #[test]
    fn md5_after_final_starts_from_zero() {
        assert_eq!(md5_from([0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476], b"abc").to_vec(), h("900150983cd24fb0d6963f7d28e17f72"));
        assert_eq!(md5_from([0; 4], b"abc").to_vec(), h("c974bc19183bcf1b909e7f07ea1e8fbb"));
    }

    #[test]
    fn blocks_and_stream_match_polcore() {
        let iv = [0xde, 0xc0, 0xad, 0x0b, 0xce, 0xfa, 0xed, 0xfe];
        let mut bf = PolBlowfish::new(&h("0001020304050607"), iv);
        assert_eq!(bf.block(0, 0), (0x82ca_c3ea, 0x338c_f935));
        assert_eq!(bf.block(0x0123_4567, 0x89ab_cdef), (0xe7aa_29d5, 0x3800_a019));
        let mut d = h("4e49434b2054657374636861723a303132333435363738396162636465660d0a");
        bf.stream(&mut d, true);
        assert_eq!(d, h("376b71111913c09a0e12fb61c95d1f298170406b57f36d1b48a1fffbf8fe0d0a"));
        let mut d = h("55534552206120622063203a640d0a");
        bf.stream(&mut d, false);
        assert_eq!(d, h("c99c93f92ee4375afa51cd844b0d0a"));
        let mut bf = PolBlowfish::new(&h("a1b2c3d4e5f60718"), [0xef, 0xcd, 0xab, 0x89, 0x67, 0x45, 0x23, 0x01]);
        let mut z = vec![0u8; 64];
        bf.stream(&mut z, true);
        assert_eq!(z, h("5ad7dcb95eb2005f6255e9e90f1f7afd94aa41ec1e81cbfaeb113b5dd3232750f6a7c5074d1327451fc7a356a6c44e8a7635b8312964bc894165c6730eea3366"));
    }

    /// The profile service's use: header reset, body continued (scratch/pol/profile/vectors.json).
    #[test]
    fn profile_stream_vectors() {
        let mut bf = PolBlowfish::new(&h("3141592653589793"), h("67452301efcdab89").try_into().unwrap());
        let mut d = h("0204050028000000000000000000000000000000000000004d69ecd7efcce1cc96148c6d7aadd09f");
        bf.stream(&mut d, true);
        assert_eq!(d, h("f0d569199f8fdf75908bd6de9647365f1af2b35f5d4134fe1e507f89cc7c44984e78d5c8bd4e2023"));
        let mut d = h("01080f161d242b323940474e555c636a71787f868d949ba2a9b0b7bec5ccd3dae1e8eff6fd040b12");
        bf.stream(&mut d, false);
        assert_eq!(d, h("589dba24da128c3273f9ec5bff3cd9ee3df35a2e7a7ecefc685411e1068683eff8037a4230f30e0e"));
    }
}
