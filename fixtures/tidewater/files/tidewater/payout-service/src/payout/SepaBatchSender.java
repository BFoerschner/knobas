package payout;

import payout.retry.RetryQueue;

public final class SepaBatchSender {
    private final RetryQueue retries = new RetryQueue();
}
