fn main() {
    let mut c: u64 = 1;
    for k in 0..=20 {
        println!("{c}");
        c = c * (20 - k) / (k + 1);
    }
}
