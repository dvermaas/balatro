//! Port of src/benchmark.py: the fixed seed sets. Never edit a set's seeds; add a new set instead.
//! Names, order and seeds copied from src/benchmark.py (SETS, NAME).

pub const NAME: &str = "bench30-a";

/// Every set, in the order of the Python dict SETS.
pub const SETS: &[(&str, &[&str])] = &[
    ("bench10-a", &[
        "7X7WSDCX", "6CAXN9I6", "BAPQJMY5", "TBIR2XMU", "PX8GK4E6", "WWYTV5PP", "DHPB8Q1L", "XUTRVN1Y", "2RGEX6EN", "V98SL5G2",
    ]),
    ("bench30-a", &[
        "7X7WSDCX", "6CAXN9I6", "BAPQJMY5", "TBIR2XMU", "PX8GK4E6", "WWYTV5PP", "DHPB8Q1L", "XUTRVN1Y", "2RGEX6EN", "V98SL5G2",
        "WMAZ7HWJ", "W4YA1M4U", "U8AC5647", "8XNG984V", "SDNUURBN", "A7YBUVWD", "X62IZYJ4", "JRE681Y8", "75WVKF42", "I9Q96AKF",
        "LRFULSLB", "8GWNFCSN", "MW6PBTW4", "5V1F9XQ2", "2S38V5NR", "2CM51JLD", "UR3ZTT24", "C3XPB74B", "JV8CL6BQ", "57AXFNDP",
    ]),
    ("balatrobench-15", &[
        "AAAAAAA#1", "AAAAAAA#2", "AAAAAAA#3", "BBBBBBB#1", "BBBBBBB#2", "BBBBBBB#3", "CCCCCCC#1", "CCCCCCC#2", "CCCCCCC#3", "DDDDDDD#1",
        "DDDDDDD#2", "DDDDDDD#3", "EEEEEEE#1", "EEEEEEE#2", "EEEEEEE#3",
    ]),
    ("weak7-a", &[
        "75WVKF42", "DHPB8Q1L", "PX8GK4E6", "JV8CL6BQ", "2RGEX6EN", "2CM51JLD", "BAPQJMY5",
    ]),
    ("sim30-a", &[
        "TU9KANLF", "MGBZKPLC", "YZAI217Z", "2YLF77B5", "1YJ4DQ2R", "XFIKZ36U", "665WDGX8", "UDBU5JA1", "11BLBC1T", "6CLKMFPY",
        "1SHJTVG7", "EV2CS4TX", "Z9DXS1JX", "6L3CIXAF", "R8ZXJN6L", "MZ5KZ3JH", "AEQ18EUN", "HXGTTFCH", "3XFPWBMH", "61QF3Z7R",
        "SXYS1LUB", "QNCJLKQ3", "X4JSXYEF", "KGA553W7", "XT9RAZDH", "4R5NYC8E", "68RZS4XR", "F1PE5WEY", "SKP9DVE3", "WKPGXL14",
    ]),
    ("sim60-a", &[
        "TU9KANLF", "MGBZKPLC", "YZAI217Z", "2YLF77B5", "1YJ4DQ2R", "XFIKZ36U", "665WDGX8", "UDBU5JA1", "11BLBC1T", "6CLKMFPY",
        "1SHJTVG7", "EV2CS4TX", "Z9DXS1JX", "6L3CIXAF", "R8ZXJN6L", "MZ5KZ3JH", "AEQ18EUN", "HXGTTFCH", "3XFPWBMH", "61QF3Z7R",
        "SXYS1LUB", "QNCJLKQ3", "X4JSXYEF", "KGA553W7", "XT9RAZDH", "4R5NYC8E", "68RZS4XR", "F1PE5WEY", "SKP9DVE3", "WKPGXL14",
        "GY2PWF7D", "Z88P7IDQ", "BIA2QXW3", "8Y1ZLY8X", "57R27KQ2", "GMJWI7Q1", "D172P6BH", "DUSJQ1TN", "MHVZ1JEC", "K792ZEJU",
        "A7MZDVCC", "Z3GELG7B", "DRMQY3JS", "IDXTTNVK", "PCL2K7FC", "XYK41G5M", "JU76QQT6", "USJXYQLN", "2HKM8YZ8", "BAF8SB49",
        "6MV2BE26", "54ATPSF9", "NSMRHDU9", "7NULGXWV", "3F5ILH3G", "D54JXDEK", "GTYIS5F4", "RS718JG4", "RHPJ6FZH", "QQ7KIM4A",
    ]),
    ("gap4-a", &[
        "XYK41G5M", "68RZS4XR", "R8ZXJN6L", "54ATPSF9",
    ]),
];

/// Seeds of a named set (sets may repeat a seed as "SEED#n"), or None for an unknown name.
pub fn set(name: &str) -> Option<&'static [&'static str]> {
    SETS.iter().find(|(n, _)| *n == name).map(|(_, s)| *s)
}
