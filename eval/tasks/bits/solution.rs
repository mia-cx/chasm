fn main() {
    for n in [1u32, 12, 255, 1024, 2147483647] {
        println!("{}", n.count_ones());
        println!("{}", 32 - n.leading_zeros());
    }
}
