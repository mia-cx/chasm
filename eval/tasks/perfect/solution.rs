fn divisor_sum(n: u32) -> u32 {
    (1..n).filter(|d| n % d == 0).sum()
}

fn main() {
    for n in 2..10000 {
        if divisor_sum(n) == n {
            println!("{n}");
        }
    }
    println!("{}", (1..1000).filter(|&n| divisor_sum(n) > n).count());
}
