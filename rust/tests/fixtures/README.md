# Numerical peak regression fixture

`cipher_peak_f32.json` contains 256 single-channel float samples (about 5.8 ms at 44.1 kHz), extracted from the 0.4.0 Louder test render of **Cipher** by **Kevin MacLeod**, incompetech.com, ISRC USUAN1100844.

Recording source: https://incompetech.com/music/royalty-free/index.html?isrc=USUAN1100844

License: Creative Commons Attribution 4.0, https://creativecommons.org/licenses/by/4.0/

The fixture is a modified numerical excerpt for automated reconstruction testing, not a full recording or a listening asset. Its JSON records the original download hash, modification and an independently computed 8×/64-tap Kaiser reconstruction peak. The older short EBU interpolation filter misses this peak. The regression verifies detection and final protection without downloading music in ordinary tests.
