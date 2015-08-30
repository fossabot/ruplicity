use regex::Regex;

//pub struct Collection {
//    pub backup_chain : Vec<BackupSet>
//}
//
//pub struct BackupSet {
//    files : Vec<FileName>
//}
//
//impl Collection {
//    pub fn open(path : &str) -> Self {
//        Collection{ backup_chain : Vec::new() }
//    }
//}

#[derive(Eq, PartialEq, Debug)]
pub enum FileType {
    //FullSig,
    //NewSig,
    //Inc,
    Full
}

#[derive(Eq, PartialEq, Debug)]
pub struct FileName {
    pub file_type : FileType,
    pub manifest : bool,
    pub volume_number : i32,
    pub time : String,
    // TODO enable those fields
    //start_time : String,
    //end_time : String,
    pub compressed : bool,
    pub encrypted : bool,
    pub partial : bool
}

impl FileName {
    /// Builder pattern for FileName
    pub fn new() -> Self {
        FileName{file_type : FileType::Full,
                 manifest : false,
                 volume_number : 0,
                 time : "".to_owned(),
                 // TODO enable those fields
                 //start_time : "".to_owned(),
                 //end_time : "".to_owned(),
                 compressed : false,
                 encrypted : false,
                 partial : false}
    }
}

gen_setters!(FileName,
    file_type : FileType,
    manifest : bool,
    volume_number : i32,
    time : String,
    //start_time : String,
    //end_time : String,
    compressed : bool,
    encrypted : bool,
    partial : bool
);


pub struct FileNameParser {
    full_vol_re : Regex,
    full_manifest_re : Regex
}

impl FileNameParser {
    pub fn new() -> Self {
        FileNameParser {
            full_vol_re : Regex::new(r"^duplicity-full\.(?P<time>.*?)\.vol(?P<num>[0-9]+)\.difftar(?P<partial>(\.part))?($|\.)").unwrap(),
            full_manifest_re : Regex::new(r"^duplicity-full\.(?P<time>.*?)\.manifest(?P<partial>(\.part))?($|\.)").unwrap()
        }
    }

    pub fn parse(&self, filename : &str) -> Option<FileName> {
        use std::ascii::AsciiExt;

        let lower_fname = filename.to_ascii_lowercase();
        let mut opt_result = self.check_full(&lower_fname);
        // write encrypted and compressed properties
        // independently of which type of file is
        if let Some(ref mut result) = opt_result {
            result.compressed = self.is_compressed(lower_fname.as_ref());
            result.encrypted = self.is_encrypted(lower_fname.as_ref());
        }
        opt_result
    }

    fn check_full(&self, filename : &str) -> Option<FileName> {
        if let Some(captures) = self.full_vol_re.captures(filename) {
            let time = captures.name("time").unwrap();
            // TODO: str2time
            let vol_num = try_opt!(self.get_vol_num(captures.name("num").unwrap()));
            return Some(FileName::new().file_type(FileType::Full)
                        .volume_number(vol_num)
                        .time(time.to_owned()));
        }
        if let Some(captures) = self.full_manifest_re.captures(filename) {
            let time = captures.name("time").unwrap();
            // TODO: str2time
            return Some(FileName::new().file_type(FileType::Full)
                        .manifest(true)
                        .time(time.to_owned())
                        .partial(captures.name("partial").is_some()));
        }
        return None;
    }

    fn get_vol_num(&self, s : &str) -> Option<i32> {
        s.parse::<i32>().ok()
    }

    fn is_encrypted(&self, s : &str) -> bool {
        s.ends_with(".gpg") || s.ends_with(".g")
    }

    fn is_compressed(&self, s : &str) -> bool {
        s.ends_with(".gz") || s.ends_with(".z")
    }
}


#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn parser_test() {
        let parser = FileNameParser::new();
        assert_eq!(parser.parse("invalid"), None);
        assert_eq!(parser.parse("duplicity-full.20150617T182545Z.vol1.difftar.gz"),
                   Some(FileName{file_type : FileType::Full,
                                 manifest : false,
                                 volume_number : 1,
                                 time : "20150617t182545z".to_owned(),
                                 compressed : true,
                                 encrypted: false,
                                 partial : false}));
    }
}
