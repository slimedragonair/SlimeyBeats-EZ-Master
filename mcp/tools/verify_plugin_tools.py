"""Exercise owner-scoped plugin access and actual rendering through MCP."""
import argparse,json,math,struct,tempfile,wave
from pathlib import Path
from protocol_client import Client

def verify(server,plugins):
    with tempfile.TemporaryDirectory(prefix='SlimeyBeats MCP plugins ') as temporary:
        root=Path(temporary);library=root/'Music';library.mkdir();source=library/'Tone.wav'
        with wave.open(str(source),'wb') as wav:
            wav.setparams((2,2,48000,0,'NONE',''))
            wav.writeframes(b''.join(struct.pack('<hh',int(math.sin(i/48000*math.tau*750)*8000),int(math.sin(i/48000*math.tau*750)*8000)) for i in range(48000)))
        client=Client([str(server),'--input-root',str(library),'--output-root',str(root/'Exports'),'--plugin-root',str(plugins)])
        try:
            items=client.call('list_plugins')['libraries'][0]['plugins'];assert items
            path=items[0]['path']
            probed=client.run('probe_plugin',{'library':0,'path':path})
            effect=next(p for p in probed['plugins'] if p.get('parameters'))
            loaded=client.run('load_audio',{'library':0,'path':source.name})
            rendered=client.run('render_plugin',{'audio_id':loaded['source_id'],'library':0,'path':path,'class_id':effect['class']['id'],'parameters':[{'id':0,'normalized':0.5}]})
            assert rendered['studio_preview_id']
            assert not list((root/'Exports').glob('*.wav'))
            assert client.call('get_session')['master'] is None
            client.call('probe_plugin',{'library':0,'path':'../outside.vst3'},error=True)
            saved=client.run('export_studio_audio',{'studio_preview_id':rendered['studio_preview_id'],'name':'Plugin result'})
            assert (root/'Exports'/saved['exports'][0]['path']).is_file()
            return {'probe':True,'real_render':True,'explicit_export':True,'scope_rejection':True}
        finally:client.close()

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--server',type=Path,required=True);parser.add_argument('--plugins',type=Path,required=True);args=parser.parse_args()
    print(json.dumps(verify(args.server.resolve(),args.plugins.resolve()),indent=2))
