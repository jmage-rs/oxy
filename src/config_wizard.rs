pub(crate) fn do_wizard(args: &::clap::ArgMatches) {
    match args.subcommand() {
        ("init-server", _matches) => {
            let mut config: crate::config::Config = Default::default();
            let mut key = [0u8; 32].to_vec();
            let resolver: ::snow::resolvers::DefaultResolver = Default::default();
            let mut rng = ::snow::resolvers::CryptoResolver::resolve_rng(&resolver).unwrap();
            ::snow::types::Random::fill_bytes(&mut *rng, &mut key);
            config.outer_key = Some(key);
            let noise_key = ::snow::Builder::new(crate::NOISE_PATTERN.parse().unwrap())
                .generate_keypair()
                .unwrap();
            config.local_private_key = Some(noise_key.private.clone());
            config.local_public_key = Some(noise_key.public.clone());
            config.mode = Some(crate::config::Mode::Server);
            config.port_number = Some(
                ::rand::Rng::gen_range(&mut ::rand::thread_rng(), 1024u16, 65535)
                    .checked_add(1)
                    .unwrap(),
            );
            print!("{}", ::toml::to_string(&config).unwrap());
        }
        _ => unimplemented!(),
    }
}
