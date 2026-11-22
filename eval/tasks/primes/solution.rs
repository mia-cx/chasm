fn main() {
    for n in 2..100 {
        if (2..n).take_while(|d| d * d <= n).all(|d| n % d != 0) {
            println!("{n}");
        }
    }
}
