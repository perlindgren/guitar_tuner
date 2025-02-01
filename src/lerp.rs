// lerp, linear interpolation

pub fn lerp_zero(y0: f32, y1: f32) -> f32 {
    -y0 / (y1 - y0)
}
#[test]
fn lerp_test() {
    println!("{}", lerp_zero(-1.0, 1.0));
    println!("{}", lerp_zero(-1.0, 2.0));
    println!("{}", lerp_zero(-2.0, 0.0));

    println!("{}", lerp_zero(1.0, -1.0));
    println!("{}", lerp_zero(1.0, -2.0));
    println!("{}", lerp_zero(2.0, 0.0));

    println!("{}", lerp_zero(2.0, 1.0));
}
