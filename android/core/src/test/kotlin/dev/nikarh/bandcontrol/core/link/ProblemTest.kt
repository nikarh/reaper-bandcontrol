package dev.nikarh.bandcontrol.core.link

import dev.nikarh.bandcontrol.core.protocol.ClientException
import dev.nikarh.bandcontrol.proto.AuthFailure
import org.junit.Assert.assertEquals
import org.junit.Test
import java.io.EOFException
import java.net.ConnectException
import java.net.SocketTimeoutException

class ProblemTest {
    @Test
    fun failuresMapToProblems() {
        assertEquals(Problem.Lost, problemOf(EOFException(), connected = true))
        assertEquals(Problem.Unknown, problemOf(EOFException(), connected = false))
        assertEquals(Problem.Unreachable, problemOf(ConnectException("refused"), connected = false))
        assertEquals(Problem.Unreachable, problemOf(SocketTimeoutException(), connected = false))
        assertEquals(Problem.Lost, problemOf(SocketTimeoutException(), connected = true))
        assertEquals(Problem.NotFound, problemOf(LinkException(Problem.NotFound, "scan"), connected = false))
        assertEquals(Problem.PermissionMissing, problemOf(SecurityException(), connected = false))
        assertEquals(Problem.PairingRejected, problemOf(ClientException.BadServerProof(), connected = false))
        assertEquals(
            Problem.PairingRejected,
            problemOf(ClientException.AuthFailed(AuthFailure.AUTH_FAILURE_BAD_PROOF), connected = false),
        )
        assertEquals(
            Problem.Incompatible,
            problemOf(ClientException.AuthFailed(AuthFailure.AUTH_FAILURE_UNSUPPORTED_VERSION), connected = false),
        )
        assertEquals(
            Problem.TooManyPhones,
            problemOf(ClientException.AuthFailed(AuthFailure.AUTH_FAILURE_TOO_MANY_CLIENTS), connected = false),
        )
        assertEquals(Problem.Incompatible, problemOf(ClientException.Protocol("bad"), connected = false))
        assertEquals(Problem.Unknown, problemOf(IllegalStateException(), connected = false))
    }
}
