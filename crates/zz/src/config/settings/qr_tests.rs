use super::*;

#[test]
fn qr_layout_clears_centered_bird_halo_without_touching_finders_or_timing() {
    let short = qr_layout("zz://host/a:9922").expect("short url encodes");
    assert_eq!(short.width, 29);
    assert_eq!(short.bird_origin, (9, 11));
    assert!(short.bird_origin.0 - QR_BIRD_HALO > 7);
    assert!(short.bird_origin.1 - QR_BIRD_HALO > 7);

    let host = qr_layout("zz://host/macbook.beluga-wall.ts.net:9922").expect("host encodes");
    assert_eq!(host.width, 33);
    assert_eq!(host.bird_origin, (11, 13));
    assert!(
        host.modules
            .iter()
            .all(|&(x, y)| !((10..22).contains(&x) && (12..20).contains(&y)))
    );
    for (fx, fy) in [(0, 0), (host.width - 7, 0), (0, host.width - 7)] {
        assert!(host.modules.contains(&(fx, fy)));
        assert!(host.modules.contains(&(fx + 3, fy + 3)));
    }
}
