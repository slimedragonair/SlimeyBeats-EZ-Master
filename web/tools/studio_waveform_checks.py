"""Pixel and repeated-read checks using actual AudioBuffer and Canvas APIs."""


def verify_waveform(page):
    result = page.evaluate('''async()=>{
        const url=[...performance.getEntriesByType('resource')].find(entry=>entry.name.includes('/studio-plots.js')).name;
        const {waveform}=await import(url),context=new AudioContext(),buffer=context.createBuffer(2,96000,48000);
        buffer.getChannelData(1).fill(.5);
        let reads=0;const original=buffer.getChannelData.bind(buffer);
        buffer.getChannelData=(channel)=>{reads++;return original(channel);};
        const canvas=document.querySelector('#s-wave'),playback={before:buffer,after:buffer,position:()=>0};
        waveform(canvas,playback);const firstReads=reads;
        waveform(canvas,playback);const repeatedReads=reads-firstReads;
        const pixels=canvas.getContext('2d').getImageData(0,0,canvas.width,canvas.height).data;
        let visible=0;
        for(let x=100;x<canvas.width-10;x++)for(let y=20;y<31;y++){
            if(Math.abs(y-25)<2)continue;const offset=(y*canvas.width+x)*4;
            if(pixels[offset]+pixels[offset+1]+pixels[offset+2]>160)visible++;
        }
        await context.close();
        return {right_only_trace_pixels:visible,first_reads:firstReads,repeated_reads:repeatedReads};
    }''')
    assert result['right_only_trace_pixels'] > 100, result
    assert result['first_reads'] == 2 and result['repeated_reads'] == 0, result
    return result
