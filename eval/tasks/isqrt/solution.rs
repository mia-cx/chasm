fn main() {
    for n in [0u64, 1, 15, 16, 1000000, 2147483647] {
        let mut r = 0;
        while (r + 1) * (r + 1) <= n {
            r += 1;
        }
        println!("{r}");
    }
}
