#!/usr/bin/env python3
"""Execute the checkout's actual generated Java bridge encoder; no JVM in Rust tests."""
import argparse
import hashlib
import json
from pathlib import Path
from reference_sources import reference_checkout, source_path
import re
import shutil
import subprocess
import tempfile
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
REFERENCE_ROOT, REFERENCE_COMMIT = reference_checkout(ROOT)
SOURCE = REFERENCE_ROOT / 'server/desktop/src/main/java/dev/slimevr/desktop/platform/ProtobufMessages.java'
FIXTURES = ROOT / 'server-rust/crates/slimevr-server/tests/fixtures'
HARNESS = r'''
import dev.slimevr.desktop.platform.ProtobufMessages.*;
import java.util.HexFormat;
class BridgeGolden {
 static void emit(String name, ProtobufMessage message) {
   System.out.println(name + " " + HexFormat.of().formatHex(message.toByteArray()));
 }
 public static void main(String[] args) {
   emit("version", ProtobufMessage.newBuilder().setVersion(Version.newBuilder().setProtocolVersion(2)).build());
   for (int id=0;id<3;id++) {
     emit("added"+id, ProtobufMessage.newBuilder().setTrackerAdded(TrackerAdded.newBuilder().setTrackerId(id)
       .setTrackerSerial("OpenVR/"+id).setTrackerName(id==0?"HMD":"Controller")
       .setTrackerRole(id==0?19:id==1?13:14).setManufacturer("Valve")).build());
     emit("pose"+id, ProtobufMessage.newBuilder().setPosition(Position.newBuilder().setTrackerId(id)
       .setX(id==0?0f:id==1?-.3f:.3f).setY(id==0?1.7f:1.1f).setZ(id==0?0f:-.4f)
       .setQw(1f).setQx(0f).setQy(0f).setQz(0f).setDataSource(Position.DataSource.FULL)
       .setVx(0f).setVy(-.25f).setVz(.5f)).build());
   }
   emit("rotation_only", ProtobufMessage.newBuilder().setPosition(Position.newBuilder().setTrackerId(0).setQw(1f)).build());
   for (var status: TrackerStatus.Status.values()) {
     if(status==TrackerStatus.Status.UNRECOGNIZED) continue;
     emit("status_"+status.name(), ProtobufMessage.newBuilder().setTrackerStatus(TrackerStatus.newBuilder()
       .setTrackerId(1).setStatus(status).setConfidence(TrackerStatus.Confidence.HIGH)).build());
   }
   emit("battery", ProtobufMessage.newBuilder().setBattery(Battery.newBuilder().setTrackerId(2).setBatteryLevel(.65f).setIsCharging(true)).build());
   for (String action: new String[]{"reset","fast_reset","mounting_reset","feet_mounting_reset","pause_tracking"}) {
     emit(action, ProtobufMessage.newBuilder().setUserAction(UserAction.newBuilder().setName(action).putActionArguments("source","controller")).build());
   }
   emit("empty", ProtobufMessage.newBuilder().build());
 }
}
'''

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--javac-java-modules', type=Path)
    parser.add_argument('--cache', type=Path, default=Path(tempfile.gettempdir()) / 'slimevr-steamvr-oracle')
    args = parser.parse_args()
    args.cache.mkdir(parents=True, exist_ok=True)
    jar = args.cache / 'protobuf-java-4.31.1.jar'
    if not jar.exists():
        urllib.request.urlretrieve('https://repo.maven.apache.org/maven2/com/google/protobuf/protobuf-java/4.31.1/protobuf-java-4.31.1.jar', jar)
    classes = args.cache / 'classes'
    classes.mkdir(exist_ok=True)
    harness = args.cache / 'BridgeGolden.java'
    harness.write_text(HARNESS)
    if shutil.which('javac'):
        javac = [shutil.which('javac')]
    elif args.javac_java_modules:
        javac = ['java','--module-path', str(args.javac_java_modules), '--add-modules','jdk.compiler','-m','jdk.compiler/com.sun.tools.javac.Main']
    else:
        parser.error('JDK javac or --javac-java-modules is required')
    subprocess.run([*javac, '-cp', str(jar), '-d', str(classes), str(SOURCE), str(harness)], check=True)
    output = subprocess.check_output(['java','-cp',f'{classes}{__import__("os").pathsep}{jar}','BridgeGolden'], text=True)
    samples = [{'name': line.partition(' ')[0], 'hex': line.partition(' ')[2]} for line in output.splitlines()]
    # Extract the same embedded descriptor executed by the original Java class.
    import codecs
    block = SOURCE.read_text().split('String[] descriptorData = {',1)[1].split('};',1)[0]
    descriptor = ''.join(codecs.decode(s, 'unicode_escape') for s in re.findall(r'"((?:\\.|[^"\\])*)"',block)).encode('latin1')
    (FIXTURES / 'steamvr-java-descriptor.pb').write_bytes(descriptor)
    result = {'source_sha256':hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
              'protobuf_java_sha256':hashlib.sha256(jar.read_bytes()).hexdigest(), 'samples':samples}
    (FIXTURES / 'steamvr-java-golden.json').write_text(json.dumps(result,indent=2)+'\n')
    print(f'Generated {len(samples)} actual Java bridge messages')

if __name__ == '__main__':
    main()
