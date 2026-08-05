pub fn clenshaw_q16(c: &[i32; 8], x_q: i32) -> i32 {
    let mut bk2: i32 = 0;
    let mut bk1: i32 = 0;
    let two_q: i32 = 2 << 16; // 2.0 in Q1.16

    // Iterate k = 7 down to 1
    for k in (1..=7).rev() {
        // term = 2 * x_q * bk1  (all Q1.16, result Q1.16 via >> 16 from i64)
        // Use i64 multiply to hold the intermediate: 2*x_q*bk1
        // 2(Q16) * x(Q16) * bk1(Q16) → Q48, shift right 32 to get Q16
        let term = ((two_q as i64 * x_q as i64 * bk1 as i64) >> 32) as i32;
        let bk = c[k] + term - bk2;
        bk2 = bk1;
        bk1 = bk;
    }

    // Final step: c[0] + x * bk1 - bk2  (x is Q16, bk1 is Q16, single 32-bit factor)
    let term_f = ((x_q as i64 * bk1 as i64) >> 16) as i32;
    c[0] + term_f - bk2
}
