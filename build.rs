// build.rs

fn main() {
    cc::Build::new()
        .file("src/udpx/recvmsg.c")
        .compile("recvmsg");
}
