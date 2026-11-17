// Diagnostic MD5 is a content identifier, NOT a security/integrity primitive.
// Incremental blocks avoid a whole-file allocation; yield after each 256 KiB.
const K = Uint32Array.from({ length: 64 }, (_, i) => Math.floor(Math.abs(Math.sin(i + 1)) * 4294967296));
const S = [7,12,17,22, 5,9,14,20, 4,11,16,23, 6,10,15,21];
export class DiagnosticMd5 {
    constructor() { this.state = new Uint32Array([0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476]); this.tail = new Uint8Array(64); this.used = 0; this.bytes = 0; }
    block(data, offset) {
        const words = new Uint32Array(16), view = new DataView(data.buffer, data.byteOffset + offset, 64);
        for (let i = 0; i < 16; i++) words[i] = view.getUint32(i * 4, true);
        let [a,b,c,d] = this.state;
        for (let i = 0; i < 64; i++) {
            const r = i >> 4, g = r === 0 ? i : r === 1 ? (5*i+1)%16 : r === 2 ? (3*i+5)%16 : (7*i)%16;
            const f = r === 0 ? (b&c)|(~b&d) : r === 1 ? (d&b)|(~d&c) : r === 2 ? b^c^d : c^(b|~d);
            const n = (a + f + K[i] + words[g]) >>> 0, shift = S[r*4+i%4];
            [a,b,c,d] = [d, (b + ((n << shift) | (n >>> (32-shift)))) >>> 0, b, c];
        }
        [a,b,c,d].forEach((v,i) => { this.state[i] = (this.state[i] + v) >>> 0; });
    }
    update(data) {
        this.bytes += data.length; let offset = 0;
        if (this.used) { const count = Math.min(64-this.used, data.length); this.tail.set(data.subarray(0,count),this.used); this.used += count; offset += count; if (this.used===64) { this.block(this.tail,0); this.used=0; } }
        for (; offset+64<=data.length; offset+=64) this.block(data,offset);
        if (offset<data.length) { this.tail.set(data.subarray(offset)); this.used=data.length-offset; }
        return this;
    }
    digest() {
        const bytes = this.bytes, pad = new Uint8Array(this.used < 56 ? 64-this.used : 128-this.used); pad[0]=128;
        const view = new DataView(pad.buffer); view.setUint32(pad.length-8,(bytes*8)>>>0,true); view.setUint32(pad.length-4,Math.floor(bytes/536870912),true); this.update(pad);
        return [...this.state].map(v => [0,8,16,24].map(s => ((v>>>s)&255).toString(16).padStart(2,'0')).join('')).join('');
    }
}
export function detectTraceFormat(bytes) {
    const tag = (at,len) => String.fromCharCode(...bytes.subarray(at,at+len));
    if (bytes.length >= 12 && tag(0,4)==='RIFF' && tag(8,4)==='WAVE') {
        const v = new DataView(bytes.buffer,bytes.byteOffset,bytes.byteLength);
        for (let p=12; p+8<=bytes.length;) {
            const size=v.getUint32(p+4,true);
            if (tag(p,4)==='fmt ' && size>=16 && p+24<=bytes.length) {
                let code=v.getUint16(p+8,true); const bits=v.getUint16(p+22,true);
                if (code===65534 && size>=40 && p+48<=bytes.length) {
                    const guid = [...bytes.subarray(p+34,p+48)];
                    if (guid.join(',') === '0,0,0,0,16,0,128,0,0,170,0,56,155,113') code=v.getUint16(p+32,true);
                }
                if (code===3 && [32,64].includes(bits)) return `wav ${bits}f`;
                if (code===1 && [8,16,24,32].includes(bits)) return `wav ${bits}pcm`;
                return 'wav unknown_encoding';
            }
            if (size>bytes.length || p+8+size<=p) break; p+=8+size+(size&1);
        }
        return 'wav unknown_encoding';
    }
    if (tag(0,4)==='fLaC') return 'flac';
    if (tag(0,4)==='OggS') return 'ogg unknown_codec';
    if (bytes[0]===0x50 && bytes[1]===0x4b && bytes[2]===3 && bytes[3]===4) return 'zip';
    return 'unknown'; // Never infer encoding from a filename or MIME claim.
}
export async function fileTraceMetadata(file, { yieldTask = () => new Promise(resolve => setTimeout(resolve,0)), cancelled = () => false } = {}) {
    const size = file.size, header = new Uint8Array(await file.slice(0,65536).arrayBuffer());
    const format = detectTraceFormat(header);
    if (size > 1024*1024*1024) return { size, format, file_hash: null, hash_status: 'skipped_size_limit' };
    const md5 = new DiagnosticMd5();
    for (let p=0; p<size; p+=262144) {
        if (cancelled()) return { size, format, file_hash: null, hash_status: 'cancelled' };
        md5.update(new Uint8Array(await file.slice(p,p+262144).arrayBuffer())); await yieldTask();
    }
    return { size, format, file_hash: md5.digest(), hash_algorithm: 'md5', hash_status: 'complete' };
}
