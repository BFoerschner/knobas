module com.example.demo {
    requires transitive java.logging;
    requires static java.sql;
    exports com.example.demo;
    opens com.example.demo to java.logging;
    uses java.lang.Runnable;
}
