// A SQL runner for TeamCity's internal HSQLDB, for seed-scale.sh teamcity's
// date rewrite: statements on stdin, one per line, a query's rows printed tab
// separated. Run with the server STOPPED (HSQLDB's file lock), in the server's
// own image, which ships a JDK and hsqldb-jdk8.jar:
//
//   java -cp .../hsqldb-jdk8.jar Sql.java jdbc:hsqldb:file:<system>/buildserver < statements
//
// TeamCity's REST takes no build dates; this is the one way to spread the
// scale builds over the two years the rest of the estate covers.
import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.sql.Connection;
import java.sql.DriverManager;
import java.sql.ResultSet;
import java.sql.Statement;

public class Sql {
    public static void main(String[] args) throws Exception {
        try (Connection c = DriverManager.getConnection(args[0] + ";shutdown=true", "SA", "")) {
            c.setAutoCommit(false);
            BufferedReader in = new BufferedReader(new InputStreamReader(System.in));
            String line;
            int changed = 0;
            try (Statement s = c.createStatement()) {
                while ((line = in.readLine()) != null) {
                    line = line.trim();
                    if (line.isEmpty() || line.startsWith("--")) continue;
                    if (s.execute(line)) {
                        try (ResultSet r = s.getResultSet()) {
                            int n = r.getMetaData().getColumnCount();
                            while (r.next()) {
                                StringBuilder b = new StringBuilder();
                                for (int k = 1; k <= n; k++) b.append(k > 1 ? "\t" : "").append(r.getString(k));
                                System.out.println(b);
                            }
                        }
                    } else {
                        changed += s.getUpdateCount();
                    }
                }
            }
            c.commit();
            try (Statement s = c.createStatement()) { s.execute("CHECKPOINT"); }
            System.err.println("sql: " + changed + " rows changed");
        }
    }
}
