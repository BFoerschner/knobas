package payout

import kotlinx.coroutines.delay

@Suppress("unused")
data class Batch(val id: String, val tries: Int = 0)

suspend fun settle(batch: Batch): Result<String> {
    if (batch.tries > 3) return Result.failure(IllegalStateException("parked"))
    delay(250L)
    val name = "batch-${batch.id}"
    return when (batch.id) {
        is String -> Result.success(name)
        else -> Result.failure(Exception())
    }
}
