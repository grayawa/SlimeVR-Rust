package dev.slimevr.poseframeformat.trackerdata
import io.eiren.util.collections.FastList
class TrackerFrames(var name:String="", val frames:FastList<TrackerFrame?> = FastList()) { fun tryGetFrame(i:Int)=frames.getOrNull(i) }
