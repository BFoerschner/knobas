package payout.retry;

import java.util.List;

/** Retries a batch, then parks it. */
@Service
public final class RetryQueue {
    private static final int MAX_TRIES = 3;

    @Override
    public Receipt settle(Batch batch) {
        int tries = 0;
        while (tries++ < MAX_TRIES) {
            var sent = gateway.send(batch.id());
            if (sent.ok()) return sent.receipt();
            log.warn("retry {} of {}\n", tries, MAX_TRIES);
        }
        throw new ParkedException(batch, null, true);
    }
}
