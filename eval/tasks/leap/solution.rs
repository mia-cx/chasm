fn main() {
    for year in [1900, 1996, 2000, 2023, 2024, 2100] {
        println!("{year}");
        let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
        println!("{}", if leap { "leap" } else { "common" });
    }
}
