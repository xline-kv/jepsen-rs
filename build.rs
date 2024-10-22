use j4rs::{JvmBuilder, MavenArtifact, MavenArtifactRepo, MavenSettings};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Only rerun when build.rs changes, saves a lot of time
    println!("cargo:rerun-if-changed=build.rs");

    let jvm = JvmBuilder::new()
        .with_maven_settings(MavenSettings::new(vec![
            MavenArtifactRepo::from("maven_central:https://repo1.maven.org/maven2"),
            MavenArtifactRepo::from("clojars::https://repo.clojars.org"),
        ]))
        .build()?;

    // Since j4rs doesn't support recursive downloads from Maven, the
    // following dependencies are generated and converted from Lein.
    //
    // Top level dependencies:
    // * jepsen 0.3.5
    // * elle 0.2.1
    // * cheshire/cheshire "5.11.0"
    let artifacts = [
        "org.clojure:clojure:1.12.0",
        "elle:elle:0.2.1",
        "com.aphyr:bifurcan-clj:0.1.1",
        "dom-top:dom-top:1.0.9",
        "riddley:riddley:0.2.0",
        "rhizome:rhizome:0.2.9",
        "jepsen:jepsen:0.3.5",
        "byte-streams:byte-streams:0.2.5-alpha2",
        "clj-tuple:clj-tuple:0.2.2",
        "manifold:manifold:0.1.8",
        "io.aleph:dirigiste:0.1.5",
        "primitive-math:primitive-math:0.1.6",
        "clj-ssh:clj-ssh:0.5.14",
        "com.jcraft:jsch.agentproxy.core:0.0.9",
        "com.jcraft:jsch.agentproxy.jsch:0.0.9",
        "com.jcraft:jsch.agentproxy.pageant:0.0.9",
        "com.jcraft:jsch.agentproxy.sshagent:0.0.9",
        "com.jcraft:jsch.agentproxy.usocket-jna:0.0.9",
        "net.java.dev.jna:jna-platform:4.1.0",
        "net.java.dev.jna:jna:4.1.0",
        "com.jcraft:jsch.agentproxy.usocket-nc:0.0.9",
        "com.jcraft:jsch:0.1.53",
        "clj-time:clj-time:0.15.2",
        "joda-time:joda-time:2.10",
        "com.hierynomus:sshj:0.38.0",
        "com.hierynomus:asn-one:0.6.0",
        "net.i2p.crypto:eddsa:0.3.0",
        "org.bouncycastle:bcpkix-jdk18on:1.75",
        "org.bouncycastle:bcutil-jdk18on:1.75",
        "org.bouncycastle:bcprov-jdk18on:1.75",
        "org.slf4j:slf4j-api:2.0.7",
        "com.jcraft:jsch.agentproxy.connector-factory:0.0.9",
        "com.jcraft:jsch.agentproxy.sshj:0.0.9",
        "fipp:fipp:0.6.26",
        "org.clojure:core.rrb-vector:0.1.2",
        "gnuplot:gnuplot:0.1.3",
        "hiccup:hiccup:1.0.5",
        "http-kit:http-kit:2.7.0",
        "io.jepsen:history:0.1.3",
        "io.lacuna:bifurcan:0.2.0-alpha7",
        "potemkin:potemkin:0.4.7",
        "tesser.core:tesser.core:1.0.6",
        "jepsen.txn:jepsen.txn:0.1.2",
        "knossos:knossos:0.3.10",
        "com.boundary:high-scale-lib:1.0.6",
        "interval-metrics:interval-metrics:1.0.1",
        "org.clojars.pallix:analemma:1.0.0",
        "org.clojure:math.combinatorics:0.2.0",
        "metametadata:multiset:0.1.1",
        "org.clojure:algo.generic:0.1.2",
        "org.bouncycastle:bcprov-jdk15on:1.70",
        "org.clojure:data.codec:0.1.1",
        "org.clojure:data.fressian:1.0.0",
        "org.fressian:fressian:0.6.6",
        "org.clojure:tools.cli:1.0.219",
        "org.clojure:tools.logging:1.2.4",
        "ring:ring:1.11.0",
        "org.ring-clojure:ring-jakarta-servlet:1.11.0",
        "ring:ring-core:1.11.0",
        "commons-io:commons-io:2.15.0",
        "crypto-equality:crypto-equality:1.0.1",
        "crypto-random:crypto-random:1.2.1",
        "commons-codec:commons-codec:1.15",
        "org.apache.commons:commons-fileupload2-core:2.0.0-M1",
        "org.ring-clojure:ring-websocket-protocols:1.11.0",
        "ring:ring-codec:1.2.0",
        "ring:ring-devel:1.11.0",
        "clj-stacktrace:clj-stacktrace:0.2.8",
        "ns-tracker:ns-tracker:0.4.0",
        "org.clojure:java.classpath:0.3.0",
        "org.clojure:tools.namespace:0.2.11",
        "ring:ring-jetty-adapter:1.11.0",
        "org.eclipse.jetty.websocket:websocket-jetty-server:11.0.18",
        "org.eclipse.jetty.websocket:websocket-jetty-api:11.0.18",
        "org.eclipse.jetty.websocket:websocket-jetty-common:11.0.18",
        "org.eclipse.jetty.websocket:websocket-core-common:11.0.18",
        "org.eclipse.jetty.websocket:websocket-servlet:11.0.18",
        "org.eclipse.jetty.websocket:websocket-core-server:11.0.18",
        "org.eclipse.jetty:jetty-servlet:11.0.18",
        "org.eclipse.jetty:jetty-security:11.0.18",
        "org.eclipse.jetty:jetty-webapp:11.0.18",
        "org.eclipse.jetty:jetty-xml:11.0.18",
        "org.eclipse.jetty:jetty-server:11.0.18",
        "org.eclipse.jetty.toolchain:jetty-jakarta-servlet-api:5.0.2",
        "org.eclipse.jetty:jetty-http:11.0.18",
        "org.eclipse.jetty:jetty-util:11.0.18",
        "org.eclipse.jetty:jetty-io:11.0.18",
        "slingshot:slingshot:0.12.2",
        "spootnik:unilog:0.7.31",
        "ch.qos.logback:logback-classic:1.4.4",
        "ch.qos.logback:logback-core:1.4.4",
        "com.fasterxml.jackson.core:jackson-annotations:2.14.0-rc2",
        "com.fasterxml.jackson.core:jackson-core:2.14.0-rc2",
        "com.fasterxml.jackson.core:jackson-databind:2.14.0-rc2",
        "net.logstash.logback:logstash-logback-encoder:7.2",
        "org.slf4j:jcl-over-slf4j:2.0.3",
        "org.slf4j:jul-to-slf4j:2.0.3",
        "org.slf4j:log4j-over-slf4j:2.0.3",
        "nrepl:nrepl:1.0.0",
        "org.clojure:clojure:1.11.3",
        "org.clojure:core.specs.alpha:0.2.62",
        "org.clojure:spec.alpha:0.3.218",
        "org.nrepl:incomplete:0.1.0",
        "org/clojure:pom.contrib:0.2.2",
        // cheshire
        "cheshire:cheshire:5.11.0",
        "org.clojure:core.specs.alpha:0.4.74",
        "org.clojure:spec.alpha:0.5.238",
        "org.clojure:pom.contrib:1.2.0",
        "com.fasterxml.jackson.dataformat:jackson-dataformat-smile:2.13.3",
        "com.fasterxml.jackson.core:jackson-core:2.13.3",
        "com.fasterxml.jackson.dataformat:jackson-dataformat-cbor:2.13.3",
        "tigris:tigris:0.1.2",
        "com.fasterxml.jackson.dataformat:jackson-dataformats-binary:2.13.3",
        "com.fasterxml.jackson:jackson-base:2.13.3",
        "com.fasterxml.jackson:jackson-bom:2.13.3",
        "com.fasterxml.jackson:jackson-parent:2.13",
        "com.fasterxml.oss-parent:43",
    ];

    for artifact in artifacts {
        let mvn_artifact = MavenArtifact::from(artifact);
        jvm.deploy_artifact(&mvn_artifact)?;
    }

    Ok(())
}
