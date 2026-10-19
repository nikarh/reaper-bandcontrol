package dev.nikarh.bandcontrol.core.link

import dev.nikarh.bandcontrol.core.protocol.ClientException
import dev.nikarh.bandcontrol.proto.AuthFailure
import java.io.IOException
import java.net.ConnectException
import java.net.NoRouteToHostException
import java.net.SocketTimeoutException
import java.net.UnknownHostException

/** Why there is no connection, in terms the UI can explain without technical details. */
enum class Problem {
    BluetoothUnavailable,
    BluetoothOff,
    PermissionMissing,

    /** Nothing with the paired service UUID answered the scan. */
    NotFound,

    /** Found (or addressed), but the connection could not be established. */
    Unreachable,

    /** Connected, but the server never finished the handshake. */
    NoAnswer,

    /** The connection dropped after it worked. */
    Lost,
    PairingRejected,
    TooManyPhones,

    /** App and extension speak different protocol versions. */
    Incompatible,
    Unknown,
}

/** A connection failure with a known [problem]; the message is technical and only meant for logs. */
class LinkException(
    val problem: Problem,
    message: String,
    cause: Throwable? = null,
) : IOException(message, cause)

/** Maps a failure to a [Problem]. [connected] tells whether the session had been authenticated before it failed. */
fun problemOf(
    error: Throwable,
    connected: Boolean,
): Problem =
    when (error) {
        is LinkException -> {
            error.problem
        }

        is ClientException.AuthFailed -> {
            when (error.reason) {
                AuthFailure.AUTH_FAILURE_UNSUPPORTED_VERSION -> Problem.Incompatible
                AuthFailure.AUTH_FAILURE_TOO_MANY_CLIENTS -> Problem.TooManyPhones
                AuthFailure.AUTH_FAILURE_RATE_LIMITED -> Problem.NoAnswer
                else -> Problem.PairingRejected
            }
        }

        is ClientException.BadServerProof -> {
            Problem.PairingRejected
        }

        is ClientException.Protocol -> {
            if (connected) Problem.Lost else Problem.Incompatible
        }

        is SecurityException -> {
            Problem.PermissionMissing
        }

        is ConnectException, is NoRouteToHostException, is UnknownHostException -> {
            Problem.Unreachable
        }

        is SocketTimeoutException -> {
            if (connected) Problem.Lost else Problem.Unreachable
        }

        is IOException -> {
            if (connected) Problem.Lost else Problem.Unknown
        }

        else -> {
            Problem.Unknown
        }
    }
