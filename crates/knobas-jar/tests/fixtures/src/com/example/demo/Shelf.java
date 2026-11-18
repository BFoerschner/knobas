package com.example.demo;

import java.io.IOException;
import java.util.List;
import java.util.Map;

/** A shelf of books: generics, annotations, nested and inner classes, a constant, varargs, throws. */
@Deprecated(since = "2.0", forRemoval = true)
public abstract class Shelf<T extends Comparable<T>> implements Iterable<T>, java.io.Serializable {
    public static final int CAPACITY = 42;
    public static final String LABEL = "front \"row\"";
    public static final long BIG = 1L << 40;
    public static final double RATIO = 0.5;
    protected transient volatile Map<String, List<? extends T>> byTag;
    private final long[] counts = new long[0];

    public Shelf(String name, int size) {}

    @SuppressWarnings("unchecked")
    public abstract <R extends T> List<R> find(String query, int limit) throws IOException, InterruptedException;

    public static synchronized int sum(int... values) { int s = 0; for (int v : values) s += v; Runnable r = () -> {}; r.run(); return s; }

    public enum Colour { RED, GREEN; public String lower() { return name().toLowerCase(); } }

    public record Book(String title, @Deprecated int pages) implements Comparable<Book> {
        public int compareTo(Book o) { return title.compareTo(o.title); }
    }

    public sealed interface Shape permits Circle, Square { double area(); default String kind() { return "shape"; } static Shape unit() { return new Circle(1); } }
    public static final class Circle implements Shape { final double r; Circle(double r) { this.r = r; } public double area() { return r * r * Math.PI; } }
    public static non-sealed class Square implements Shape { public double area() { return 1; } }

    public @interface Tag { String value(); int weight() default 1; Colour colour() default Colour.RED; Class<?>[] types() default {}; }

    class Inner { void touch() {} }

    class Holder<V> { Holder(V value, int n) {} }
}
