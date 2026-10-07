package dev.slimevr.config
import com.fasterxml.jackson.databind.node.ObjectNode
class SkeletonConfig { var userHeight=0f }
class OracleVRConfig(val pose:ObjectNode, val autoBone:AutoBoneConfig) { val skeleton=SkeletonConfig() }
class ConfigManager(val vrConfig:OracleVRConfig)
