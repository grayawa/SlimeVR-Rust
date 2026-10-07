import com.fasterxml.jackson.databind.JsonNode
import dev.slimevr.config.StayAlignedConfig
import dev.slimevr.math.Angle
import dev.slimevr.tracking.processor.skeleton.HumanSkeleton
import dev.slimevr.tracking.processor.stayaligned.StayAligned
import dev.slimevr.tracking.processor.stayaligned.trackers.TrackerSkeleton
import dev.slimevr.tracking.trackers.*
import dev.slimevr.tracking.trackers.udp.IMUType
import dev.slimevr.VRServer
import kotlin.time.*
object OracleTimeSource:AbstractLongTimeSource(DurationUnit.NANOSECONDS) {override fun read()=OracleClock.nanos}
fun alignmentReference(n:JsonNode):Any {
    OracleClock.nanos=0
    val config=StayAlignedConfig();config.enabled=true;config.standingRelaxedPose.enabled=true;config.sittingRelaxedPose.enabled=true;config.flatRelaxedPose.enabled=true
    if(n.has("relaxed")) for(p in arrayOf(config.standingRelaxedPose,config.sittingRelaxedPose,config.flatRelaxedPose)) {
        p.upperLegAngleInDeg=n["relaxed"]["upper_leg_degrees"].floatValue()
        p.lowerLegAngleInDeg=n["relaxed"]["lower_leg_degrees"].floatValue()
        p.footAngleInDeg=n["relaxed"]["foot_degrees"].floatValue()
    }
    val skeleton=HumanSkeleton()
    val tracks=n["frames"][0]["rotations"].properties().associate{e->TrackerPosition.valueOf(e.key.uppercase()) to Tracker(TrackerPosition.valueOf(e.key.uppercase())).apply{imuType=IMUType.LSM6DSV;directRotation=quaternion(e.value)}}
    skeleton.trackers=tracks
    val topology=TrackerSkeleton(skeleton)
    var last=0L
    return n["frames"].map {frame->
        val at=frame["at_ms"].longValue();OracleClock.nanos=at*1000000
        for ((body,t) in tracks){t.directRotation=quaternion(frame["rotations"][body.name.lowercase()]);t.raw=t.directRotation!!;t.stayAligned.update()}
        VRServer.instance.fpsTimer.timePerFrame=(at-last)/1000f;last=at
        StayAligned.adjustNextTracker(topology,config)
        tracks.entries.associate{(body,t)->body.name.lowercase() to mapOf("correction" to t.stayAligned.yawCorrection.toRad(),"rest" to t.stayAligned.restDetector.state.name.lowercase())}
    }
}
