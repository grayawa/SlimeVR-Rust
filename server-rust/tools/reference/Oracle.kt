import dev.slimevr.tracking.trackers.udp.*
import io.github.axisangles.ktmath.Quaternion
import io.github.axisangles.ktmath.Vector3
import java.nio.ByteBuffer

private fun q(q: Quaternion) = linkedMapOf("w" to q.w, "x" to q.x, "y" to q.y, "z" to q.z)
private fun v(v: Vector3) = linkedMapOf("x" to v.x, "y" to v.y, "z" to v.z)
private fun json(value: Any?): String = when(value) {
    null -> "null"
    is String -> "\"" + value.replace("\\", "\\\\").replace("\"", "\\\"").replace("\n", "\\n") + "\""
    is Map<*,*> -> value.entries.joinToString(",", "{", "}") {json(it.key) + ":" + json(it.value)}
    is Iterable<*> -> value.joinToString(",", "[", "]") {json(it)}
    else -> value.toString()
}
private fun hex(buf: ByteBuffer) = buf.array().take(buf.position()).joinToString("") {"%02x".format(it)}
private fun packet(p: UDPPacket): Map<String, Any?> = when(p) {
    is UDPPacket3Handshake -> mapOf("type" to "handshake", "info" to mapOf("board_type" to p.boardType.id.toLong(), "imu_type" to p.imuType.id.toLong(), "mcu_type" to p.mcuType.id.toLong(), "protocol_version" to p.protocolVersion, "firmware" to p.firmware, "mac" to p.macString))
    is UDPPacket15SensorInfo -> mapOf("type" to "sensor_info", "info" to mapOf("sensor_id" to p.sensorId, "status" to p.sensorStatus, "imu_type" to p.sensorType.id.toLong(), "config" to p.sensorConfig?.v?.toInt(), "rest_calibrated" to p.hasCompletedRestCalibration, "body_position" to p.trackerPosition?.id, "data_type" to p.trackerDataType.id.toLong()))
    is RotationPacket -> mapOf("type" to "rotation", "sensor_id" to p.sensorId, "rotation" to q(p.rotation), "data_type" to 1, "calibration" to null, "acceleration" to if(p is UDPPacket23RotationAndAcceleration) v(p.acceleration) else null)
    is UDPPacket17RotationData -> mapOf("type" to "rotation", "sensor_id" to p.sensorId, "rotation" to q(p.rotation), "data_type" to p.dataType, "calibration" to p.calibrationInfo, "acceleration" to null)
    is UDPPacket4Acceleration -> mapOf("type" to "acceleration", "sensor_id" to p.sensorId, "acceleration" to v(p.acceleration))
    is UDPPacket12BatteryLevel -> mapOf("type" to "battery", "voltage" to p.voltage, "fraction" to p.level)
    is UDPPacket19SignalStrength -> mapOf("type" to "signal", "sensor_id" to p.sensorId, "rssi" to p.signalStrength)
    is UDPPacket20Temperature -> mapOf("type" to "temperature", "sensor_id" to p.sensorId, "temperature" to p.temperature)
    is UDPPacket10PingPong -> mapOf("type" to "ping", "id" to p.pingId)
    is UDPPacket21UserAction -> mapOf("type" to "user_action", "action" to p.type)
    is UDPPacket13Tap -> mapOf("type" to "tap", "sensor_id" to p.sensorId, "tap" to p.tap.tapBits)
    is UDPPacket14Error -> mapOf("type" to "error", "sensor_id" to p.sensorId, "code" to p.errorNumber)
    is UDPPacket24AckConfigChange -> mapOf("type" to "config_ack", "sensor_id" to p.sensorId, "config_type" to p.configType.v.toInt())
    is UDPPacket26FlexData -> mapOf("type" to "flex", "sensor_id" to p.sensorId, "value" to p.flexData)
    is UDPPacket27Position -> mapOf("type" to "position", "sensor_id" to p.sensorId, "position" to v(p.position))
    is UDPPacket200ProtocolChange -> mapOf("type" to "protocol_change", "protocol" to p.targetProtocol, "version" to p.targetProtocolVersion)
    is UDPPacket0Heartbeat -> mapOf("type" to "heartbeat")
    else -> error("Packet not selected for fixture: $p")
}
fun main() {
    val parser = UDPProtocolParser()
    val handshake = ByteBuffer.allocate(64); parser.writeHandshakeResponse(handshake, null)
    val ack = ByteBuffer.allocate(64); val sensor = UDPPacket15SensorInfo(sensorStatus=1); sensor.sensorId=1
    parser.writeSensorInfoResponse(ack, null, sensor)
    val flags = ByteBuffer.allocate(64); parser.write(flags, null, UDPPacket22FeatureFlags())
    println(json(mapOf("handshake" to hex(handshake), "sensor_info" to hex(ack), "features" to hex(flags))))
    for(line in generateSequence(::readLine)) {
        val bytes = line.chunked(2).map {it.toInt(16).toByte()}.toByteArray()
        val packets = parser.parse(ByteBuffer.wrap(bytes), null).filterNotNull()
        val rotations = packets.mapNotNull { when(it) {is RotationPacket -> it.rotation; is UDPPacket17RotationData -> it.rotation; else -> null} }
        val axes = Quaternion.fromRotationVector(-Math.PI.toFloat()/2f, 0f, 0f)
        val correction = Quaternion.rotationAroundZAxis(-Math.PI.toFloat()/2f)
        val accelerations = packets.mapNotNull { when(it) {is UDPPacket4Acceleration -> it.acceleration; is UDPPacket23RotationAndAcceleration -> it.acceleration; else -> null} }
        println(json(mapOf("packets" to packets.map(::packet), "server_rotations" to rotations.map {q(axes*it)}, "legacy_accelerations" to accelerations.map {v(correction.sandwich(it))})))
    }
}
