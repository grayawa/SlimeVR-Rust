import com.fasterxml.jackson.databind.JsonNode
import com.fasterxml.jackson.databind.ObjectMapper
import dev.slimevr.VRServer
import dev.slimevr.config.ResetsConfig
import dev.slimevr.filtering.*
import dev.slimevr.tracking.trackers.*
import io.github.axisangles.ktmath.*

private val mapper=ObjectMapper()
fun q(q:Quaternion)=linkedMapOf("w" to q.w,"x" to q.x,"y" to q.y,"z" to q.z)
fun v(v:Vector3)=linkedMapOf("x" to v.x,"y" to v.y,"z" to v.z)
fun quaternion(n:JsonNode)=Quaternion(n["w"].floatValue(),n["x"].floatValue(),n["y"].floatValue(),n["z"].floatValue())
fun vector(n:JsonNode)=Vector3(n["x"].floatValue(),n["y"].floatValue(),n["z"].floatValue())
fun filterMode(n:JsonNode)=TrackerFilters.valueOf(n.asText().uppercase())

fun main() {
    for(line in generateSequence(::readLine)) {
        val n=mapper.readTree(line)
        val output:Any=when(n["kind"].asText()) {
            "math" -> {
                val a=quaternion(n["a"]);val b=quaternion(n["b"]);val x=vector(n["vector"]);val t=n["t"].floatValue()
                val yaw=Quaternion.rotationAroundYAxis(a.toEulerAngles(EulerOrder.YZX).y).twinNearest(a)
                mapOf("mul" to q(a*b),"inverse" to q(a.inv()),"unit" to q(a.unit()),"pow" to q(a.pow(t)),"interp_q" to q(a.interpQ(b,t)),"interp_r" to q(a.interpR(b,t)),"twin_nearest" to q(a.twinNearest(b)),"twin_extended_back" to q(a.twinExtendedBack(b)),"project" to q(a.project(Vector3.POS_Y)),"yaw_yzx" to q(yaw),"euler_yxz" to a.toEulerAngles(EulerOrder.YXZ).let{mapOf("x" to it.x,"y" to it.y,"z" to it.z)},"rotate" to v(a.sandwich(x)),"angle" to a.angleToR(b))
            }
            "filter" -> {
                val filter=QuaternionMovingAverage(filterMode(n["mode"]),n["amount"].floatValue(),quaternion(n["initial"]))
                n["steps"].map {step ->
                    when(step["op"].asText()) {
                        "sample" -> filter.addQuaternion(quaternion(step["q"]))
                        "tick" -> {VRServer.instance.fpsTimer.timePerFrame=step["dt"].floatValue();filter.update()}
                        "reset" -> filter.resetQuats(quaternion(step["q"]),quaternion(step["reference"]))
                    }
                    mapOf("rotation" to q(filter.filteredQuaternion),"impact" to filter.filteringImpact)
                }
            }
            "calibration" -> {
                val tracker=Tracker(TrackerPosition.valueOf(n["body"].asText().uppercase()))
                if(n.path("computed").asBoolean(false)){tracker.isComputed=true;tracker.allowMounting=false}
                val resets=tracker.resetsHandler;resets.mountingOrientation=quaternion(n["mounting"])
                val config=ResetsConfig();if(n.has("arms_mode"))config.mode=dev.slimevr.config.ArmsResetModes.valueOf(n["arms_mode"].asText().uppercase());config.yawResetSmoothTime=n["smooth_seconds"].floatValue();resets.readResetConfig(config)
                n["steps"].map {step ->
                    when(step["op"].asText()) {
                        "sample" -> {tracker.raw=quaternion(step["q"]);tracker.filteringHandler.average.addQuaternion(resets.getReferenceAdjustedDriftRotationFrom(tracker.raw))}
                        "full" -> resets.resetFull(quaternion(step["reference"]))
                        "yaw" -> resets.resetYaw(quaternion(step["reference"]))
                        "mounting" -> resets.resetMounting(quaternion(step["reference"]))
                        "tick" -> tracker.yawResetSmoothing.tick(step["dt"].floatValue())
                    }
                    mapOf("adjusted" to q(resets.getReferenceAdjustedDriftRotationFrom(tracker.raw)),"mount_rot_fix" to q(resets.mountRotFix),"transition" to q(tracker.yawResetSmoothing.curRotation))
                }
            }
            "flex" -> {
                val tracker=Tracker(TrackerPosition.valueOf(n["body"].asText().uppercase()))
                val flex=TrackerFlexHandler(tracker)
                n["steps"].map {step ->
                    when(step["op"].asText()){
                        "resistance"->flex.setFlexResistance(step["value"].floatValue())
                        "angle"->flex.setFlexAngle(step["value"].floatValue())
                        "min"->flex.resetMin()
                        "max"->flex.resetMax()
                    }
                    q(tracker.getRotation())
                }
            }
            "velocity" -> VelocityReference().run(n)
            "hmd_calibration" -> {
                val tracker=Tracker(TrackerPosition.HEAD).apply{isHmd=true;isComputed=true;allowMounting=false}
                val config=ResetsConfig()
                n["steps"].map { step ->
                    if(step.has("rotation"))tracker.raw=quaternion(step["rotation"])
                    if(step.path("full").asBoolean(false)) {
                        config.resetHmdPitch=step.path("enabled").asBoolean(false)
                        tracker.resetsHandler.readResetConfig(config)
                        tracker.resetsHandler.resetFull(Quaternion.IDENTITY)
                    }
                    q(tracker.resetsHandler.getReferenceAdjustedDriftRotationFrom(tracker.raw))
                }
            }
            "frame_orders" -> FrameOrdersReference.run(n["count"].intValue(),n["epochs"].intValue(),n["seed"].longValue())
            "alignment" -> alignmentReference(n)
            "legs" -> legReference(n)
            "skeleton" -> SkeletonReference().run(n)
            "skeleton_sequence" -> {val skeleton=SkeletonReference();n["frames"].map {skeleton.run(it)}}
            else -> error("unknown reference case")
        }
        println(mapper.writeValueAsString(output))
    }
}
