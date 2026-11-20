package payout.retry;

class RetryQueueTest {
    void aRefusedPayoutWaitsOnTheRetryQueue() {
        new RetryQueue().offer("p-1");
    }
}
