fn main() {
    let (mut a, mut b) = (0u64, 1u64);
    for _ in 0..=46 {
        println!("{a}");
        (a, b) = (b, a + b);
    }
}
