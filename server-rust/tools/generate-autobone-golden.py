#!/usr/bin/env python3
"""Execute upstream AutoBone training methods, iterator, objectives, statistics and FK.

Server/filesystem callbacks and recording containers are explicit adapters. Actual
HumanSkeleton and bone-offset methods are extracted from the checkout, not ported
into the oracle. This does not run the full Java service or read PFS/PFR files.
"""
import copy
import argparse
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
HERE=Path(__file__).resolve().parent
ROOT=HERE.parent.parent
SOURCE=ROOT/'server/core/src/main/java'
OFFSET_FIELDS=dict(HEAD='head_shift',NECK='neck_length',UPPER_CHEST='upper_chest_length',CHEST='chest_length',CHEST_OFFSET='chest_offset',WAIST='waist_length',HIP='hip_length',HIP_OFFSET='hip_offset',HIPS_WIDTH='hips_width',UPPER_LEG='upper_leg_length',LOWER_LEG='lower_leg_length',FOOT_LENGTH='foot_length',FOOT_SHIFT='foot_shift',SKELETON_OFFSET='skeleton_offset',SHOULDERS_DISTANCE='shoulders_distance',SHOULDERS_WIDTH='shoulders_width',UPPER_ARM='upper_arm_length',LOWER_ARM='lower_arm_length',HAND_Y='hand_y',HAND_Z='hand_z',ELBOW_OFFSET='elbow_offset')
METHODS=['loadConfigValues','applyConfig','calcTargetHmdHeight','updateRecordingScale','filterFrames','processFrames','epoch','step','sumAdjustedHeightOffsets','sumHeightOffsets','scaleSkeleton','scaleOffsets','getErrorDeriv']
def extract(text,name):
    start=re.search(r'\t(?:(?:private|protected) )?fun '+name+r'\(',text).start()
    opening=text.index('{',start);depth=1;end=opening+1
    while depth:
        if text[end]=='{':depth+=1
        if text[end]=='}':depth-=1
        end+=1
    return text[start:end]
def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cache',type=Path,default=Path(tempfile.gettempdir())/'slimevr-udp-oracle')
    parser.add_argument('--javac-java-modules',type=Path)
    parser.add_argument('--skip-core-build',action='store_true',help='Reuse previously generated Kotlin core classes')
    parser.add_argument('--output',type=Path,default=HERE.parent/'crates/slimevr-core/tests/fixtures/autobone-golden.json')
    args=parser.parse_args()
    spec=importlib.util.spec_from_file_location('core_generator',HERE/'generate-core-golden.py');mod=importlib.util.module_from_spec(spec);spec.loader.exec_module(mod)
    if not args.skip_core_build:
        command=['python3',str(HERE/'generate-core-golden.py'),'--cache',str(args.cache)]
        if args.javac_java_modules: command+=['--javac-java-modules',str(args.javac_java_modules)]
        subprocess.run(command,check=True)
    classes=args.cache/'autobone-classes';classes.mkdir(exist_ok=True)
    core=args.cache/'core-classes';assert core.is_dir(),'Generate core references first'
    cp=os.pathsep.join(map(str,[classes,core,*sorted((args.cache/'lib').glob('*.jar'))]))
    javac=[shutil.which('javac')] if shutil.which('javac') else ['java','--module-path',str(args.javac_java_modules),'--add-modules','jdk.compiler','-m','jdk.compiler/com.sun.tools.javac.Main']
    java=[SOURCE/p for p in ['dev/slimevr/tracking/processor/config/SkeletonConfigOffsets.java','io/eiren/util/collections/FastList.java','io/eiren/util/collections/RemoveAtSwapList.java','io/eiren/util/collections/ResettableIterator.java','io/eiren/util/collections/SkipIterator.java']]
    subprocess.run([*javac,'-cp',cp,'-d',str(classes),*map(str,java)],check=True)
    source=SOURCE/'dev/slimevr/autobone/AutoBone.kt';text=source.read_text()
    fields=text[text.index('\t// This is filled'):text.index('\n\tprivate fun loadConfigValues')]
    inner=text[text.index('\tinner class Epoch('):text.index('\tcompanion object {')]
    companion=text[text.index('\t\t// Mean square error function'):].rsplit('\n}',1)[0]
    generated='''package dev.slimevr.autobone
import dev.slimevr.autobone.errors.*
import dev.slimevr.config.*
import dev.slimevr.poseframeformat.PoseFrames
import dev.slimevr.tracking.processor.HumanPoseManager
import dev.slimevr.tracking.processor.config.*
import dev.slimevr.tracking.trackers.TrackerRole
import io.eiren.util.collections.FastList
import io.eiren.util.logging.LogManager
import io.github.axisangles.ktmath.Vector3
import java.util.*
import java.util.function.Consumer
import java.util.function.Function
import kotlin.math.*
class OracleServer(val configManager:ConfigManager,val humanPoseManager:HumanPoseManager)
class AutoBone(private val server:OracleServer) {
'''+fields+'\n'+'\n'.join(extract(text,name) for name in METHODS)+inner+'\ncompanion object { const val MIN_HEIGHT=.4f;const val MIN_SLIDE_DIST=.002f\n'+companion+'\n}\n'
    ab=args.cache/'AutoBone.training.kt';ab.write_text(generated)
    manager=SOURCE/'dev/slimevr/tracking/processor/config/SkeletonConfigManager.kt'
    height=re.search(r'val HEIGHT_OFFSETS: Array<SkeletonConfigOffsets> = arrayOf\([\s\S]*?\n\t\t\)',manager.read_text())[0]
    manager_generated='''package dev.slimevr.tracking.processor.config
import dev.slimevr.tracking.processor.BoneType
import io.github.axisangles.ktmath.Vector3
class SkeletonConfigManager(autoUpdate:Boolean) {
 val values=mutableMapOf<SkeletonConfigOffsets,Float>()
 val nodes=mutableMapOf<BoneType,Vector3>()
 fun getOffset(offset:SkeletonConfigOffsets)=values[offset] ?: offset.defaultValue
 fun setNodeOffset(type:BoneType,x:Float,y:Float,z:Float) {nodes[type]=Vector3(x,y,z)}
'''+extract(manager.read_text(),'phalanxLengthDivider')+extract(manager.read_text(),'computeNodeOffset')+'''companion object {private const val PHALANX_SUM=4.6f;private const val PROXIMAL_RATIO=2.3f;private const val INTERMEDIATE_RATIO=1.3f;private const val DISTAL_RATIO=1f
'''+height+'}\n}\n'
    scm=args.cache/'SkeletonConfigManager.offsets.kt';scm.write_text(manager_generated)
    mapping='''package dev.slimevr.tracking.processor
import dev.slimevr.tracking.processor.config.SkeletonConfigOffsets
import dev.slimevr.tracking.trackers.TrackerRole
val OFFSET_FIELDS=mapOf(\n'''+',\n'.join(f'SkeletonConfigOffsets.{key} to "{value}"' for key,value in OFFSET_FIELDS.items())+')\nval ROLE_BONES=mapOf(\n'+',\n'.join(f'TrackerRole.{role} to BoneType.{name.upper()}_TRACKER' for role,name in [('HEAD','head'),('CHEST','chest'),('WAIST','hip'),('LEFT_KNEE','left_knee'),('RIGHT_KNEE','right_knee'),('LEFT_FOOT','left_foot'),('RIGHT_FOOT','right_foot'),('LEFT_ELBOW','left_elbow'),('RIGHT_ELBOW','right_elbow'),('LEFT_HAND','left_hand'),('RIGHT_HAND','right_hand')])+')\n'
    maps=args.cache/'AutoBoneMapping.kt';maps.write_text(mapping)
    sources=[SOURCE/p for p in ['dev/slimevr/config/AutoBoneConfig.kt','dev/slimevr/autobone/AutoBoneStep.kt','dev/slimevr/autobone/StatsCalculator.kt','dev/slimevr/autobone/PoseFrameIterator.kt','dev/slimevr/autobone/PoseFrameStep.kt','dev/slimevr/autobone/BoneContribution.kt','dev/slimevr/poseframeformat/player/TrackerFramesPlayer.kt','dev/slimevr/poseframeformat/player/PlayerTracker.kt']]
    sources+=sorted((SOURCE/'dev/slimevr/autobone/errors').rglob('*.kt'))
    sources+=sorted((HERE/'autobone-reference').glob('*.kt'))+[ab,scm,maps]
    subprocess.run(['java','-cp',cp,'org.jetbrains.kotlin.cli.jvm.K2JVMCompiler','-Xvalue-classes','-no-stdlib','-no-reflect','-jvm-target','17','-classpath',cp,'-d',str(classes),*map(str,sources)],check=True)
    selected=cases(mod)
    result=subprocess.run(['java','-cp',cp,'AutoBoneOracleKt'],input=''.join(json.dumps(c)+'\n' for c in selected),text=True,capture_output=True)
    if result.returncode: raise RuntimeError(result.stderr)
    expected=[json.loads(line) for line in result.stdout.splitlines()];assert len(expected)==len(selected)
    hashes={str(p.relative_to(ROOT)):hashlib.sha256(p.read_bytes()).hexdigest() for p in [*java,source,manager,*sources] if p.is_relative_to(ROOT)}
    hashes.update({'generated/'+p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in [ab,scm,maps]})
    fixture=dict(reference_commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),scope=__doc__,source_sha256=hashes,core_fixture_sha256=hashlib.sha256((HERE.parent/'crates/slimevr-core/tests/fixtures/core-golden.json').read_bytes()).hexdigest(),extracted_methods=METHODS,cases=[dict(**c,expected=e) for c,e in zip(selected,expected)])
    args.output.write_text(json.dumps(fixture,indent=2)+'\n');print(f'Generated {len(selected)} complete AutoBone training references: {args.output}')
def cases(mod):
    frames=[]
    for i in range(16):
        t=i*.31
        rotations={name:mod.quat((1,0,0),angle) for name,angle in [('chest',.2*math.sin(t)),('hip',.1*math.sin(t+.2)),('left_upper_leg',-.45*math.sin(t)),('right_upper_leg',-.3*math.sin(t+.4)),('left_lower_leg',.5*math.sin(t)),('right_lower_leg',.4*math.sin(t+.4))]}
        rotations['upper_chest']=rotations['chest'];rotations['left_foot']=mod.quat();rotations['right_foot']=mod.quat()
        head=dict(rotation=mod.quat(angle=.1*math.sin(t)),position=mod.vector(.02*math.sin(t),1.65-.05*abs(math.sin(t)),.04*math.cos(t)))
        positions=dict(upper_chest=mod.vector(.01,1.25-.04*math.sin(t),-.03),chest=mod.vector(0,1.2,0),waist=mod.vector(0,1.,0),hip=mod.vector(.01,.95,-.02),left_foot=mod.vector(-.13,.03*math.sin(t),.02),right_foot=mod.vector(.13,.02*math.cos(t),-.01))
        frames.append(dict(at_ms=i*20,rotations=rotations,head=head,positions=positions))
    out=[]
    for name,changes in [('default',{}),('initial_random',dict(calc_initial_error=True)),('all_objectives',dict(offset_slide_factor=.2,foot_height_factor=.3,height_factor=.1,position_factor=.2,position_offset_factor=.1)),('estimate_height',dict(scale_each_step=False)),('filter_frames',dict(filter_outliers=True)),('reject',dict(max_final_error=0.000001))]:
        cfg=dict(epochs=4,cursor_increment=2,min_distance=1,max_distance=2,initial_adjust_rate=10.,adjust_rate_decay=1.,randomize=True,seed=4,scale_each_step=True,filter_outliers=False,slide_factor=1.,offset_slide_factor=0.,foot_height_factor=0.,proportion_factor=.05,height_factor=0.,position_factor=0.,position_offset_factor=0.,max_final_error=.2,calc_initial_error=False,use_skeleton_height=False)
        cfg.update(changes)
        out.append(dict(name=name,initial=mod.skeleton_config(enforce_constraints=False),frames=frames,target_height=1.65,config=cfg))
    out.append(dict(name='constraints',initial=mod.skeleton_config(enforce_constraints=True),frames=frames,target_height=1.65,config=out[0]['config']))
    for name, changes in [('recorded_height', dict(randomize=False)), ('skeleton_height', dict(use_skeleton_height=True))]:
        out.append(dict(name=name, initial=out[0]['initial'], frames=frames, target_height=None, config=dict(out[0]['config'], **changes)))
    for name,missing in [('missing_left_shin',['left_lower_leg']),('missing_shins',['left_lower_leg','right_lower_leg']),('one_leg',['right_upper_leg','right_lower_leg','right_foot']),('torso_only',['left_upper_leg','right_upper_leg','left_lower_leg','right_lower_leg','left_foot','right_foot']),('head_only',list(frames[0]['rotations']))]:
        sparse=copy.deepcopy(frames)
        for frame in sparse:
            for body in missing: frame['rotations'].pop(body,None)
        out.append(dict(name=name,initial=out[0]['initial'],frames=sparse,target_height=1.65,config=out[0]['config']))
    return out
if __name__=='__main__':main()
