import com.fasterxml.jackson.databind.ObjectMapper
import dev.slimevr.poseframeformat.*
import dev.slimevr.poseframeformat.trackerdata.*
import dev.slimevr.tracking.trackers.TrackerPosition
import java.io.*
private val mapper=ObjectMapper()
fun main() {
 for(line in generateSequence(::readLine)) {
  val n=mapper.readTree(line);val recording=PoseFrames();recording.frameInterval=n["interval"].floatValue()
  for(t in n["trackers"]) {val tracker=TrackerFrames(t["name"].textValue());recording.frameHolders.add(tracker)
   for(f in t["frames"])tracker.frames.add(TrackerFrame(f["body"]?.takeUnless{it.isNull}?.let{TrackerPosition.valueOf(it.textValue().uppercase())}, f["rotation"]?.takeUnless{it.isNull}?.let{quaternion(it)},f["position"]?.takeUnless{it.isNull}?.let{vector(it)},f["acceleration"]?.takeUnless{it.isNull}?.let{vector(it)},f["raw_rotation"]?.takeUnless{it.isNull}?.let{quaternion(it)}))
  }
  val pfs=ByteArrayOutputStream().also{PfsIO.writeFrames(DataOutputStream(it),recording)}.toByteArray()
  val pfr=ByteArrayOutputStream().also{PfrIO.writeFrames(DataOutputStream(it),recording)}.toByteArray()
  check(PfsIO.readFrames(DataInputStream(ByteArrayInputStream(pfs))).maxFrameCount==recording.maxFrameCount)
  check(PfrIO.readFrames(DataInputStream(ByteArrayInputStream(pfr))).maxFrameCount==recording.maxFrameCount)
  println(mapper.writeValueAsString(mapOf("pfs" to pfs.joinToString(""){"%02x".format(it)},"pfr" to pfr.joinToString(""){"%02x".format(it)})))
 }
}
