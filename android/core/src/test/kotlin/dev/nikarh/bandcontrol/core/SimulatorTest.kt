package dev.nikarh.bandcontrol.core

import dev.nikarh.bandcontrol.core.link.TcpConnector
import dev.nikarh.bandcontrol.core.protocol.PairingSecret
import dev.nikarh.bandcontrol.core.state.BandState
import dev.nikarh.bandcontrol.core.state.groups
import dev.nikarh.bandcontrol.proto.PlayState
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Test
import java.io.File
import java.net.ServerSocket
import java.net.Socket

/** Kotlin client against the Rust simulator (`bandctl sim`) over TCP. Skipped when bandctl is not built. */
class SimulatorTest {
    private val secret = PairingSecret(ByteArray(16) { it.toByte() })
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private lateinit var sim: Process
    private var tcpPort = 0
    private var controlPort = 0

    private fun freePort() = ServerSocket(0).use { it.localPort }

    @Before
    fun startSimulator() {
        val bandctl = File(System.getProperty("bandcontrol.bandctl") ?: "")
        assumeTrue("bandctl not built: $bandctl", bandctl.canExecute())
        tcpPort = freePort()
        controlPort = freePort()
        sim =
            ProcessBuilder(
                bandctl.path,
                "sim",
                "--name",
                "Simulator",
                "--secret",
                secret.toCode(),
                "--tcp",
                "127.0.0.1:$tcpPort",
                "--control",
                "127.0.0.1:$controlPort",
            ).redirectErrorStream(true)
                .redirectOutput(File.createTempFile("bandctl-sim", ".log"))
                .start()
        val deadline = System.currentTimeMillis() + 10_000
        while (runCatching { Socket("127.0.0.1", controlPort).close() }.isFailure) {
            check(System.currentTimeMillis() < deadline) { "simulator did not start" }
            Thread.sleep(50)
        }
    }

    @After
    fun stopSimulator() {
        scope.cancel()
        if (::sim.isInitialized) sim.destroy()
    }

    private fun control(command: String): String =
        Socket("127.0.0.1", controlPort).use { s ->
            s.getOutputStream().write("$command\n".toByteArray())
            s.getInputStream().bufferedReader().readLine()
        }

    private fun <T> StateFlow<T>.await(predicate: (T) -> Boolean): T = runBlocking { withTimeout(10_000) { first(predicate) } }

    private fun connection(secret: PairingSecret = this.secret) =
        Connection(scope, TcpConnector("127.0.0.1", tcpPort, maxPacket = 64), secret, "JVM test").also { it.start() }

    @Test
    fun fullSession() {
        val c = connection()
        val status = c.status.await { it is ConnectionStatus.Connected } as ConnectionStatus.Connected
        assertEquals("Simulator", status.serverName)
        val initial = c.state.await(BandState::loaded)
        assertEquals(0.42f, initial.tracks.single { it.name == "Drums [3+4]" }.volume)
        assertEquals(listOf("Summer", "Autumn", "Winter", "Spring"), initial.regions.map { it.name })

        assertTrue(c.send(Commands.play))
        c.state.await { it.transport.state == PlayState.PLAY_STATE_PLAYING }
        assertEquals("Playing", JSONObject(control("dump")).getJSONObject("transport").getString("play"))

        assertTrue(c.send(Commands.trackVolume(13, 0.25f)))
        c.state.await { it.track(13)?.volume == 0.25f }

        assertTrue(c.send(Commands.receiveMute(13, 0, 2, true)))
        c.state.await {
            it
                .track(13)
                ?.receives
                ?.get(0)
                ?.mute == true
        }

        // Changes on the "REAPER" side.
        assertEquals("ok", control("volume 14 0.5"))
        c.state.await { it.track(14)?.volume == 0.5f }
        assertEquals("ok", control("rename 2 Klick"))
        c.state.await { it.track(2)?.name == "Klick" }

        assertTrue(c.send(Commands.goto(3, 6)))
        c.state.await { it.transport.pos_ms in 684_522..690_000 }
    }

    @Test
    fun bandProjectGroupsAndMeters() {
        assertEquals("ok", control("reset band"))
        val c = connection()
        val state = c.state.await { s -> s.tracks.any { it.name == "INPUTS" } }
        val groups = state.groups()
        assertEquals(
            listOf("Drums input", "Bass input", "L GTR input", "R GTR input", "VOX input", "VOX BASS input", "VOX R GTR input"),
            groups.inputs.map { it.name },
        )
        assertEquals(
            listOf("Drums", "Left guitar", "Right guitar", "Bass", "Vocals"),
            groups.iemOutputs.map { it.name },
        )
        assertEquals(8, groups.paOutputs.size)
        assertEquals("PA Backtrack", groups.paOutputs.first().name)

        val drums = groups.inputs.first().id
        val bass = groups.inputs[1].id
        assertEquals("ok", control("meter $drums -3"))
        c.meter(listOf(drums, bass))
        assertEquals(mapOf(drums to -3f, bass to null), c.meters.await { it[drums] == -3f })
        assertEquals("ok", control("meter $drums 1.5"))
        c.meters.await { it[drums] == 1.5f }

        // Gain of an input track.
        assertTrue(c.send(Commands.trackVolume(drums, 2f)))
        c.state.await { it.track(drums)?.volume == 2f }

        c.meter(emptyList())
        assertEquals(emptyMap<Int, Float?>(), c.meters.value)
    }

    @Test
    fun wrongSecretIsRejected() {
        val c = connection(PairingSecret(ByteArray(16) { 9 }))
        val status = c.status.await { it is ConnectionStatus.Disconnected } as ConnectionStatus.Disconnected
        assertTrue(status.toString(), status.authRejected)
        assertTrue(!c.send(Commands.play))
    }

    @Test
    fun reconnectsAfterServerRestart() {
        val c = connection()
        c.status.await { it is ConnectionStatus.Connected }
        sim.destroy()
        sim.waitFor()
        c.status.await { it is ConnectionStatus.Disconnected }
        startSimulatorOnSamePorts()
        c.status.await { it is ConnectionStatus.Connected }
        c.state.await(BandState::loaded)
    }

    private fun startSimulatorOnSamePorts() {
        val bandctl = System.getProperty("bandcontrol.bandctl")
        sim =
            ProcessBuilder(
                bandctl,
                "sim",
                "--secret",
                secret.toCode(),
                "--tcp",
                "127.0.0.1:$tcpPort",
                "--control",
                "127.0.0.1:$controlPort",
            ).redirectErrorStream(true)
                .redirectOutput(ProcessBuilder.Redirect.DISCARD)
                .start()
    }
}
