extern crate flate2;
extern crate tar;

use std::fs::File;
use flate2::read::GzDecoder;
use tar::Archive;

#[test]
fn targz() {
    let file = File::open("tests/simple.tar.gz").unwrap();
    let gz_decoder = GzDecoder::new(file).unwrap();
    let mut tar = Archive::new(gz_decoder);
    let expected = ["a", "b", "c/", "c/d"];
    let actual: Vec<_> = tar.files_mut().unwrap()
        .map(|f| f.unwrap().filename().unwrap().to_owned())
        .collect();
    assert_eq!(actual, expected);
}
