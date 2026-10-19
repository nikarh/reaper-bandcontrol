package dev.nikarh.bandcontrol.core

import dev.nikarh.bandcontrol.core.state.NavTarget
import dev.nikarh.bandcontrol.proto.ClientMessage
import dev.nikarh.bandcontrol.proto.Goto
import dev.nikarh.bandcontrol.proto.RegionOrder
import dev.nikarh.bandcontrol.proto.SetReceive
import dev.nikarh.bandcontrol.proto.SetRegionOrder
import dev.nikarh.bandcontrol.proto.SetTrackVolume
import dev.nikarh.bandcontrol.proto.TransportAction

object Commands {
    val play = transport(TransportAction.TRANSPORT_ACTION_PLAY)
    val pause = transport(TransportAction.TRANSPORT_ACTION_PAUSE)
    val stop = transport(TransportAction.TRANSPORT_ACTION_STOP)
    val record = transport(TransportAction.TRANSPORT_ACTION_RECORD)
    val toggleRepeat = transport(TransportAction.TRANSPORT_ACTION_TOGGLE_REPEAT)

    private fun transport(action: TransportAction) = ClientMessage(transport = action)

    fun goto(
        region: Int,
        marker: Int? = null,
    ) = ClientMessage(goto = Goto(region = region, marker = marker))

    fun goto(target: NavTarget) = goto(target.region.id, target.marker?.id)

    fun trackVolume(
        track: Int,
        volume: Float,
    ) = ClientMessage(set_track_volume = SetTrackVolume(track = track, volume = volume))

    fun receiveVolume(
        track: Int,
        index: Int,
        src: Int,
        volume: Float,
    ) = ClientMessage(set_receive = SetReceive(track = track, index = index, src = src, volume = volume))

    fun receiveMute(
        track: Int,
        index: Int,
        src: Int,
        mute: Boolean,
    ) = ClientMessage(set_receive = SetReceive(track = track, index = index, src = src, mute = mute))

    fun regionOrder(order: List<RegionOrder>) = ClientMessage(set_region_order = SetRegionOrder(order = order))
}
