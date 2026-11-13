// AudioWorkletGlobalScope omits the Encoding API on some browsers. wasm-bindgen
// needs UTF-8 for its small JSON/configuration messages, never for audio blocks.
if (!globalThis.TextDecoder) {
    globalThis.TextDecoder = class {
        decode(bytes = new Uint8Array()) {
            let text = "";
            for (let i = 0; i < bytes.length; ) {
                const a = bytes[i++];
                let cp = a,
                    count = 0,
                    minimum = 0;
                if (a >= 0xc2 && a <= 0xdf) {
                    cp = a & 31;
                    count = 1;
                    minimum = 0x80;
                } else if (a >= 0xe0 && a <= 0xef) {
                    cp = a & 15;
                    count = 2;
                    minimum = 0x800;
                } else if (a >= 0xf0 && a <= 0xf4) {
                    cp = a & 7;
                    count = 3;
                    minimum = 0x10000;
                } else if (a >= 0x80) {
                    text += "\ufffd";
                    continue;
                }
                let valid = true;
                for (let j = 0; j < count; j++) {
                    if (i >= bytes.length || (bytes[i] & 0xc0) !== 0x80) {
                        valid = false;
                        break;
                    }
                    cp = (cp << 6) | (bytes[i++] & 63);
                }
                text +=
                    valid &&
                    cp >= minimum &&
                    cp <= 0x10ffff &&
                    !(cp >= 0xd800 && cp <= 0xdfff)
                        ? String.fromCodePoint(cp)
                        : "\ufffd";
            }
            return text;
        }
    };
}
if (!globalThis.TextEncoder) {
    globalThis.TextEncoder = class {
        encode(text = "") {
            const bytes = [];
            for (const character of text) {
                let c = character.codePointAt(0);
                if (c >= 0xd800 && c <= 0xdfff) c = 0xfffd;
                if (c < 128) bytes.push(c);
                else if (c < 2048) bytes.push(192 | (c >> 6), 128 | (c & 63));
                else if (c < 65536)
                    bytes.push(
                        224 | (c >> 12),
                        128 | ((c >> 6) & 63),
                        128 | (c & 63),
                    );
                else
                    bytes.push(
                        240 | (c >> 18),
                        128 | ((c >> 12) & 63),
                        128 | ((c >> 6) & 63),
                        128 | (c & 63),
                    );
            }
            return new Uint8Array(bytes);
        }
        encodeInto(text, destination) {
            let read = 0,
                written = 0;
            for (const character of text) {
                const bytes = this.encode(character);
                if (written + bytes.length > destination.length) break;
                destination.set(bytes, written);
                written += bytes.length;
                read += character.length;
            }
            return { read, written };
        }
    };
}
