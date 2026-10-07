#!/usr/bin/env python3
"""Run unchanged BVHFileStream/BVHSettings/PoseDataStream/TickReducer Kotlin sources.

Bone/HumanSkeleton adapters expose canonical FK snapshots to the real exporter.
The cached original ktmath is prepared by generate-core-golden.py; no Euler or
BVH traversal is reimplemented in this oracle. FK has its own differential suite.
"""
import argparse
import hashlib
import json
import re
from pathlib import Path
from reference_sources import reference_checkout, source_path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
REFERENCE_ROOT, REFERENCE_COMMIT = reference_checkout(ROOT)
SOURCE = REFERENCE_ROOT / 'server/core/src/main/java'
RUST = ROOT / 'server-rust'
ADAPTERS = {
    'Bone.kt': '''package dev.slimevr.tracking.processor
import io.github.axisangles.ktmath.Quaternion
import io.github.axisangles.ktmath.Vector3
class Bone(val boneType: BoneType) {
 var parent: Bone? = null
 val children = mutableListOf<Bone>()
 var rotationOffset = Quaternion.IDENTITY
 var rotation = Quaternion.IDENTITY
 var head = Vector3.NULL
 var tail = Vector3.NULL
 var length = 0f
 fun attachChild(bone: Bone) { require(bone.parent == null); children.add(bone); bone.parent = this }
 fun detachWithChildren() { children.forEach { it.parent = null }; children.clear(); parent?.children?.remove(this); parent = null }
 fun getGlobalRotation() = rotation
 fun getPosition() = head
 fun getTailPosition() = tail
}
''',
    'HumanSkeleton.kt': '''package dev.slimevr.tracking.processor.skeleton
import dev.slimevr.tracking.processor.Bone
import dev.slimevr.tracking.processor.BoneType
class HumanSkeleton(val bones: Map<BoneType, Bone>) { fun getBone(type: BoneType) = bones.getValue(type) }
''',
    'PoseStreamer.kt': '''package dev.slimevr.posestreamer
class PoseStreamer { val frameInterval = 0.01f }
''',
    'StringUtils.kt': '''package org.apache.commons.lang3
object StringUtils {
 fun repeat(s: String, n: Int) = s.repeat(n)
 fun repeat(c: Char, n: Int) = c.toString().repeat(n)
}
''',
    'BvhOracle.kt': '''import com.fasterxml.jackson.databind.ObjectMapper
import com.fasterxml.jackson.databind.JsonNode
import dev.slimevr.tracking.processor.Bone
import dev.slimevr.tracking.processor.BoneType
import dev.slimevr.tracking.processor.skeleton.HumanSkeleton
import dev.slimevr.posestreamer.BVHFileStream
import dev.slimevr.posestreamer.PoseStreamer
import dev.slimevr.util.TickReducer
import io.github.axisangles.ktmath.Quaternion
import io.github.axisangles.ktmath.Vector3
import java.io.File
fun q(n: JsonNode) = Quaternion(n["w"].floatValue(), n["x"].floatValue(), n["y"].floatValue(), n["z"].floatValue())
fun v(n: JsonNode) = Vector3(n["x"].floatValue(), n["y"].floatValue(), n["z"].floatValue())
fun skeleton(n: JsonNode, c: JsonNode): HumanSkeleton {
 val bones = n["bones"].fields().asSequence().associate { (name, pose) ->
  val type = BoneType.valueOf(name.uppercase())
  type to Bone(type).apply {
   rotationOffset = q(pose["rotation_offset"]); rotation = q(pose["rotation"])
   head = v(pose["head"]); tail = v(pose["tail"]); length = pose["length"].floatValue()
  }
 }
 val skeleton = HumanSkeleton(bones, c["left_controller"].booleanValue(), c["right_controller"].booleanValue())
 skeleton.assembleSkeleton()
 n["hierarchy"].fields().forEach { (name, links) ->
  val bone = bones.getValue(BoneType.valueOf(name.uppercase()))
  val expectedParent = if (links["parent"].isNull) null else links["parent"].asText().uppercase()
  require(bone.parent?.boneType?.name == expectedParent) { "Parent mismatch for $name" }
  require(bone.children.map { it.boneType.name } == links["children"].map { it.asText().uppercase() }) { "Children mismatch for $name" }
 }
 return skeleton
}
fun main(args: Array<String>) {
 val mapper = ObjectMapper()
 val root = mapper.readTree(File(args[0]))
 val cases = root["cases"].map { c ->
  val poses = c["poses"].map { skeleton(it, c) }
  val file = File.createTempFile("bvh-original-", ".bvh")
  try {
   BVHFileStream(file).use { stream ->
    stream.writeHeader(poses.first(), PoseStreamer())
    poses.forEach(stream::writeFrame)
    stream.writeFooter(poses.last())
   }
   mapOf("name" to c["name"].asText(), "poses" to c["poses"], "bvh" to file.readText())
  } finally { file.delete() }
 }
 val timing = root["tick_cases"].map { c ->
  var index = 0
  val fired = mutableListOf<Int>()
  val reducer = TickReducer({ fired.add(index) }, 0.01f)
  c["deltas"].forEach { reducer.tick(it.floatValue()); index++ }
  mapOf("deltas" to c["deltas"], "fired" to fired)
 }
 print(mapper.writeValueAsString(mapOf("cases" to cases, "tick_cases" to timing)))
}
''',
}

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cache', type=Path, default=Path('/tmp/slimevr-udp-oracle'))
    parser.add_argument('--cargo', default='cargo')
    parser.add_argument('--output', type=Path, default=RUST / 'crates/slimevr-server/tests/fixtures/bvh-golden.json')
    args = parser.parse_args()
    core = args.cache / 'core-classes'
    jars = list((args.cache / 'lib').glob('*.jar'))
    if not (core / 'io/github/axisangles/ktmath/Quaternion.class').exists() or not jars:
        parser.error('prepare the ktmath/Kotlin cache with generate-core-golden.py first')
    inputs = json.loads(subprocess.check_output([args.cargo, 'run', '--quiet', '--locked', '--example', 'bvh-reference-input'], cwd=RUST))
    inputs['tick_cases'] = [dict(deltas=d) for d in [
        [.004] * 250, [.003, .004, .005, .002, .010, .020, .030] * 10,
        [.0001] * 250, [0, .5, 0, .004, .004, .004, .001, .009],
    ]]
    files = [SOURCE / 'dev/slimevr/posestreamer' / (name + '.kt') for name in ['BVHFileStream', 'BVHSettings', 'PoseDataStream']]
    files.append(SOURCE / 'dev/slimevr/util/TickReducer.kt')
    human = SOURCE / 'dev/slimevr/tracking/processor/skeleton/HumanSkeleton.kt'
    source = human.read_text()
    def extract(name):
        start = source.index('fun ' + name + '(')
        end = source.index('{', start) + 1
        depth = 1
        while depth:
            if source[end] == '{': depth += 1
            if source[end] == '}': depth -= 1
            end += 1
        return source[start:end]
    fields = re.findall(r'val (\w+Bone) = Bone\(BoneType\.(\w+)', source)
    ADAPTERS['HumanSkeleton.kt'] = '''package dev.slimevr.tracking.processor.skeleton
import dev.slimevr.tracking.processor.Bone
import dev.slimevr.tracking.processor.BoneType
class HumanSkeleton(val bones: Map<BoneType, Bone>, val isTrackingLeftArmFromController: Boolean, val isTrackingRightArmFromController: Boolean) {
 fun getBone(type: BoneType) = bones.getValue(type)
 private val allArmBones get() = bones.values
''' + '\n'.join(f'val {name} get() = getBone(BoneType.{kind})' for name, kind in fields) + '\n' + extract('assembleSkeleton') + '\n' + extract('assembleSkeletonArms') + '\n}\n'
    with tempfile.TemporaryDirectory(prefix='slimevr-bvh-oracle-') as temp:
        temp = Path(temp)
        classes = temp / 'classes'
        classes.mkdir()
        adapters = []
        for name, contents in ADAPTERS.items():
            path = temp / name
            path.write_text(contents)
            adapters.append(path)
        cp = ':'.join(map(str, [*jars, core]))
        subprocess.run(['java', '-cp', cp, 'org.jetbrains.kotlin.cli.jvm.K2JVMCompiler', '-Xvalue-classes', '-no-stdlib', '-no-reflect', '-jvm-target', '17', '-classpath', cp, '-d', str(classes), *map(str, files + adapters)], check=True)
        input_path = temp / 'input.json'
        input_path.write_text(json.dumps(inputs))
        result = json.loads(subprocess.check_output(['java', '-cp', str(classes) + ':' + cp, 'BvhOracleKt', str(input_path)]))
    result['reference_commit'] = REFERENCE_COMMIT
    result['source_sha256'] = {source_path(p, ROOT, REFERENCE_ROOT): hashlib.sha256(p.read_bytes()).hexdigest() for p in files + [human]}
    result['method'] = 'Unmodified Kotlin BVH exporter and TickReducer; verbatim HumanSkeleton assembly methods; original cached ktmath; canonical FK snapshot adapters.'
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + '\n')
    print(f'Wrote {len(result["cases"])} clips to {args.output}')

if __name__ == '__main__':
    main()
