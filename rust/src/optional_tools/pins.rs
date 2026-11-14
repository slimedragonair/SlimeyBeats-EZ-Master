//! Verified 2026-11-14 against downloaded provider assets and GitHub asset digests.
//! Fixed monthly URLs, not latest. GitHub does not enforce release immutability;
//! compiled SHA-256 pins reject replacements. Provider retains monthly builds 2 years.
pub(super) const PROVIDER: &str = "https://github.com/FFmpeg-Builds/FFmpeg-Builds";
pub(super) const FFMPEG_SOURCE: &str =
    "https://github.com/FFmpeg/FFmpeg/tree/8dca1586f468bfcbb78ae3ed618af1361d9f140a";
pub(super) const BUILD_COMMIT: &str = "5031c6f1082414e53688d39e521849905071070d";
pub(super) const RELEASE: &str = "autobuild-2026-10-31-12-59";

#[derive(Clone, Copy)]
pub(super) struct Pin<'a> {
    pub platform: &'a str,
    pub url: &'a str,
    pub sha256: &'a str,
    pub prefix: &'a str,
    pub files: [&'a str; 3],
    pub hashes: [&'a str; 3],
    pub zip: bool,
    pub max_download: usize,
}

pub(super) const WINDOWS: Pin<'static> = Pin {
    platform: "windows-x86_64",
    url: "https://github.com/FFmpeg-Builds/FFmpeg-Builds/releases/download/autobuild-2026-10-31-12-59/ffmpeg-n8.1.3-27-g8dca1586f4-win64-lgpl-8.1.zip",
    sha256: "d4c2a42673917b4a97104a3e9f082729a9730796b98664e37c0b7eba6c6c613d",
    prefix: "ffmpeg-n8.1.3-27-g8dca1586f4-win64-lgpl-8.1",
    files: ["ffmpeg.exe", "ffprobe.exe", "LICENSE.txt"],
    hashes: [
        "04d0c351e53dde1cc45ba437c28d20a0b382df2798999def5d4110e23d74e62c",
        "48d6297c8e6538ca8c351300e80ea3ff67a4f0c54337f653f8df7d687ae0afbb",
        "da7eabb7bafdf7d3ae5e9f223aa5bdc1eece45ac569dc21b3b037520b4464768",
    ],
    zip: true,
    max_download: 192 * 1024 * 1024,
};

pub(super) const LINUX: Pin<'static> = Pin {
    platform: "linux-x86_64-gnu",
    url: "https://github.com/FFmpeg-Builds/FFmpeg-Builds/releases/download/autobuild-2026-10-31-12-59/ffmpeg-n8.1.3-27-g8dca1586f4-linux64-lgpl-8.1.tar.xz",
    sha256: "a64f3171e6ad1f7d358d28890a907ee478c331d1177ec696a93418f79ac3f1f3",
    prefix: "ffmpeg-n8.1.3-27-g8dca1586f4-linux64-lgpl-8.1",
    files: ["ffmpeg", "ffprobe", "LICENSE.txt"],
    hashes: [
        "09225778e63bb602f377cc9038d4556b5bd18b78d8bd5994a842ad5e262f72e3",
        "1fa3e4a30e0599d3391c83259093218d06cbd3ce7fa577d408662f5a13c7bb4a",
        "da7eabb7bafdf7d3ae5e9f223aa5bdc1eece45ac569dc21b3b037520b4464768",
    ],
    zip: false,
    max_download: 160 * 1024 * 1024,
};

pub(super) fn current() -> Option<&'static Pin<'static>> {
    if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        Some(&WINDOWS)
    } else if cfg!(all(
        target_os = "linux",
        target_arch = "x86_64",
        target_env = "gnu"
    )) {
        Some(&LINUX)
    } else {
        None
    }
}

impl Pin<'_> {
    pub fn members(&self) -> [String; 3] {
        [
            format!("{}/bin/{}", self.prefix, self.files[0]),
            format!("{}/bin/{}", self.prefix, self.files[1]),
            format!("{}/LICENSE.txt", self.prefix),
        ]
    }
}
