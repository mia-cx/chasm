fn main() {
    for mut n in [0u64, 7, 1234, 90210, 123456789] {
        let (mut sum, mut rev) = (0, 0);
        while n > 0 {
            sum += n % 10;
            rev = rev * 10 + n % 10;
            n /= 10;
        }
        println!("{sum}");
        println!("{rev}");
    }
}
