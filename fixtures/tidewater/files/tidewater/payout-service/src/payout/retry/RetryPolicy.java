package payout.retry;

/** Exponential backoff for what the RetryQueue holds: base 30 s, factor 2, at most 5 attempts. */
public record RetryPolicy(int attempts) {}
