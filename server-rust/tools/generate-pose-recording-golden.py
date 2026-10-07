#!/usr/bin/env python3
"""Compile and execute original PfsIO/PfrIO with explicit recording-container adapters."""
import argparse, hashlib, json, os, subprocess
from pathlib import Path
HERE=Path(__file__).resolve().parent;ROOT=HERE.parent.parent;SOURCE=ROOT/'server/core/src/main/java';CACHE=Path('/tmp/slimevr-udp-oracle')
def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--cache',type=Path,default=CACHE);args=p.parse_args()
 classes=args.cache/'poseio-classes';classes.mkdir(exist_ok=True)
 cp=os.pathsep.join(map(str,[classes,args.cache/'autobone-classes',args.cache/'core-classes',*sorted((args.cache/'lib').glob('*.jar'))]))
 frame=SOURCE/'dev/slimevr/poseframeformat/trackerdata/TrackerFrame.kt';text=frame.read_text();text=text[:text.index('\n\tcompanion object')]+ '\n}'
 text=text.replace('import dev.slimevr.tracking.trackers.TrackerStatus\n','').replace('import dev.slimevr.tracking.trackers.Tracker\n','')
 generated=args.cache/'TrackerFrame.io.kt';generated.write_text(text)
 sources=[SOURCE/'dev/slimevr/poseframeformat'/f for f in ['PfsIO.kt','PfrIO.kt','PfsPackets.kt','trackerdata/TrackerFrameData.kt']]+[generated,*sorted((HERE/'pose-recording-reference').glob('*.kt'))]
 subprocess.run(['java','-cp',cp,'org.jetbrains.kotlin.cli.jvm.K2JVMCompiler','-Xvalue-classes','-no-stdlib','-no-reflect','-jvm-target','17','-classpath',cp,'-d',str(classes),*map(str,sources)],check=True)
 q=dict(w=1.,x=0.,y=0.,z=0.);v=dict(x=.1,y=1.7,z=-.2)
 cases=[dict(name='all_flags_and_modified_utf',interval=.02,trackers=[dict(name='头显\0🎮',frames=[dict(body='head',rotation=q,position=v,acceleration=v,raw_rotation=q),dict(body='head',rotation=q,position=v)]),dict(name='sensor',frames=[dict(body='left_lower_arm',rotation=q),{},dict(body='right_little_distal',rotation=q)])])]
 result=subprocess.run(['java','-cp',cp,'OracleKt'],input=''.join(json.dumps(c)+'\n' for c in cases),text=True,capture_output=True,check=True)
 expected=[json.loads(l) for l in result.stdout.splitlines()]
 fixture=dict(reference_commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),scope=__doc__,source_sha256={str(f.relative_to(ROOT)):hashlib.sha256(f.read_bytes()).hexdigest() for f in [frame,*sources] if f.is_relative_to(ROOT)},generated_sha256=hashlib.sha256(generated.read_bytes()).hexdigest(),cases=[dict(**c,expected=e) for c,e in zip(cases,expected)])
 out=HERE.parent/'crates/slimevr-server/tests/fixtures/pose-recording-golden.json';out.write_text(json.dumps(fixture,indent=2)+'\n');print(f'Generated {len(cases)} original PFS/PFR recording references')
if __name__=='__main__':main()
