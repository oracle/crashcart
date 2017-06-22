error_chain! {
    types {
        Error, ErrorKind, ResultExt, Result;
    }
    foreign_links {
        Nix(::nix::Error);
        Io(::std::io::Error);
        Caps(::caps::Error);
    }
    // errors {
    //     InvalidSpec(t: String) {
    //         description("invalid spec")
    //         display("invalid spec: '{}'", t)
    //     }
    //     SeccompError(t: String) {
    //         description("seccomp spec")
    //         display("seccomp error: '{}'", t)
    //     }
    // }
}
