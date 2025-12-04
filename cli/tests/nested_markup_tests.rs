/// Integration tests for nested error markup structures
///
/// These tests verify that the ErrorMarkup struct can properly represent
/// complex nested error structures from the Python test suite.
#[cfg(test)]
mod nested_markup_tests {
    use divvun_runtime_cli::command::yaml_test::{ErrorMarkup, ErrorType, Sentence, ErrorContent, ErrorSegment};

    /// Helper to create a simple error with text content
    fn make_simple_error(
        form: &str,
        start: usize,
        end: usize,
        errortype: ErrorType,
        suggestions: Vec<&str>,
        comment: Option<&str>,
    ) -> ErrorMarkup {
        let mut error = ErrorMarkup::with_suggestions(
            form.to_string(),
            start,
            end,
            errortype,
            suggestions.iter().map(|s| s.to_string()).collect(),
        );
        if let Some(c) = comment {
            error.comment = c.to_string();
        }
        error
    }

    #[test]
    fn test_nested_errorort_inside_errormorphsyn() {
        // Input: {{šaddai}${verb,conc|šattai} ollu áššit}£{verb,fin,pl3prs,sg3prs,tense|šadde ollu áššit}
        
        // Inner error: {šaddai}${verb,conc|šattai}
        let inner_error = make_simple_error(
            "šaddai",
            0,
            "šaddai".len(),
            ErrorType::Errorort,
            vec!["šattai"],
            Some("verb,conc"),
        );

        // Outer error: {{šaddai}${verb,conc|šattai} ollu áššit}£{...}
        let mut outer_error = ErrorMarkup::new_nested(
            vec![
                ErrorSegment::Error(Box::new(inner_error)),
                ErrorSegment::Text(" ollu áššit".to_string()),
            ],
            0,
            "šaddai ollu áššit".len(),
            ErrorType::Errormorphsyn,
        );
        outer_error.comment = "verb,fin,pl3prs,sg3prs,tense".to_string();
        outer_error.suggestions = vec!["šadde ollu áššit".to_string()];

        // Verify structure
        assert_eq!(outer_error.errortype, ErrorType::Errormorphsyn);
        assert_eq!(outer_error.comment, "verb,fin,pl3prs,sg3prs,tense");
        
        match &outer_error.form {
            ErrorContent::Nested(segments) => {
                assert_eq!(segments.len(), 2);
                match &segments[0] {
                    ErrorSegment::Error(inner) => {
                        assert_eq!(inner.errortype, ErrorType::Errorort);
                        assert_eq!(inner.comment, "verb,conc");
                    }
                    _ => panic!("Expected Error segment"),
                }
            }
            _ => panic!("Expected nested content"),
        }
    }

    #[test]
    fn test_nested_generic_error_inside_errormorphsyn() {
        // Input: {guokte {ganddat}§{n,á|gánddat}}£{n,nump,gensg,nompl,case|guokte gándda}
        
        let inner_error = make_simple_error(
            "ganddat",
            7,
            7 + "ganddat".len(),
            ErrorType::Error,
            vec!["gánddat"],
            Some("n,á"),
        );

        let mut outer_error = ErrorMarkup::new_nested(
            vec![
                ErrorSegment::Text("guokte ".to_string()),
                ErrorSegment::Error(Box::new(inner_error)),
            ],
            0,
            "guokte ganddat".len(),
            ErrorType::Errormorphsyn,
        );
        outer_error.comment = "n,nump,gensg,nompl,case".to_string();
        outer_error.suggestions = vec!["guokte gándda".to_string()];

        assert_eq!(outer_error.errortype, ErrorType::Errormorphsyn);
        assert_eq!(outer_error.suggestions, vec!["guokte gándda"]);
    }

    #[test]
    fn test_nested_errorort_in_errormorphsyn_nieiddat() {
        // Input: {Nieiddat leat {nourra}${adj,meta|nuorra}}£{adj,spred,nompl,nomsg,agr|Nieiddat leat nuorat}
        
        let inner_error = make_simple_error(
            "nourra",
            14,
            14 + "nourra".len(),
            ErrorType::Errorort,
            vec!["nuorra"],
            Some("adj,meta"),
        );

        let mut outer_error = ErrorMarkup::new_nested(
            vec![
                ErrorSegment::Text("Nieiddat leat ".to_string()),
                ErrorSegment::Error(Box::new(inner_error)),
            ],
            0,
            "Nieiddat leat nourra".len(),
            ErrorType::Errormorphsyn,
        );
        outer_error.comment = "adj,spred,nompl,nomsg,agr".to_string();
        outer_error.suggestions = vec!["Nieiddat leat nuorat".to_string()];

        // Verify the structure
        match &outer_error.form {
            ErrorContent::Nested(segments) => {
                assert_eq!(segments.len(), 2);
                match &segments[1] {
                    ErrorSegment::Error(inner) => {
                        assert_eq!(inner.errortype, ErrorType::Errorort);
                        assert_eq!(inner.suggestions, vec!["nuorra"]);
                        assert_eq!(inner.comment, "adj,meta");
                    }
                    _ => panic!("Expected Error segment"),
                }
            }
            _ => panic!("Expected nested content"),
        }
    }

    #[test]
    fn test_double_nested_errormorphsyn() {
        // Input: {leat {okta máná}£{n,spred,nomsg,gensg,case|okta mánná}}£{v,v,sg3prs,pl3prs,agr|lea okta mánná}
        
        // Innermost error: {okta máná}£{...}
        let inner_error = make_simple_error(
            "okta máná",
            5,
            5 + "okta máná".len(),
            ErrorType::Errormorphsyn,
            vec!["okta mánná"],
            Some("n,spred,nomsg,gensg,case"),
        );

        // Outer error wraps "leat" + inner error
        let mut outer_error = ErrorMarkup::new_nested(
            vec![
                ErrorSegment::Text("leat ".to_string()),
                ErrorSegment::Error(Box::new(inner_error)),
            ],
            0,
            "leat okta máná".len(),
            ErrorType::Errormorphsyn,
        );
        outer_error.comment = "v,v,sg3prs,pl3prs,agr".to_string();
        outer_error.suggestions = vec!["lea okta mánná".to_string()];

        // Verify double nesting
        assert_eq!(outer_error.errortype, ErrorType::Errormorphsyn);
        match &outer_error.form {
            ErrorContent::Nested(segments) => {
                match &segments[1] {
                    ErrorSegment::Error(inner) => {
                        assert_eq!(inner.errortype, ErrorType::Errormorphsyn);
                        assert_eq!(inner.comment, "n,spred,nomsg,gensg,case");
                    }
                    _ => panic!("Expected nested Error segment"),
                }
            }
            _ => panic!("Expected nested content"),
        }
    }

    #[test]
    fn test_complex_multiple_nested_errors_in_sentence() {
        // Input: heaitit {dáhkaluddame}${verb,a|dahkaluddame} ahte sis
        // {máhkaš}¢{adv,á|mahkáš} livččii {makkarge}${adv,á|makkárge}
        // politihkka, muhto rahpasit baicca muitalivčče {{makkar}
        // ${interr,á|makkár} soga}€{man soga} sii {ovddasttit}
        // ${verb,conc|ovddastit}.
        
        // Error 1: {dáhkaluddame}${verb,a|dahkaluddame}
        let error1 = make_simple_error(
            "dáhkaluddame",
            7,
            7 + "dáhkaluddame".len(),
            ErrorType::Errorort,
            vec!["dahkaluddame"],
            Some("verb,a"),
        );

        // Error 2: {máhkaš}¢{adv,á|mahkáš}
        let error2 = make_simple_error(
            "máhkaš",
            30,
            30 + "máhkaš".len(),
            ErrorType::Errorortreal,
            vec!["mahkáš"],
            Some("adv,á"),
        );

        // Error 3: {makkarge}${adv,á|makkárge}
        let error3 = make_simple_error(
            "makkarge",
            46,
            46 + "makkarge".len(),
            ErrorType::Errorort,
            vec!["makkárge"],
            Some("adv,á"),
        );

        // Error 4 (nested): {{makkar}${interr,á|makkár} soga}€{man soga}
        let inner_error4 = make_simple_error(
            "makkar",
            95,
            95 + "makkar".len(),
            ErrorType::Errorort,
            vec!["makkár"],
            Some("interr,á"),
        );

        let mut error4 = ErrorMarkup::new_nested(
            vec![
                ErrorSegment::Error(Box::new(inner_error4)),
                ErrorSegment::Text(" soga".to_string()),
            ],
            95,
            95 + "makkar soga".len(),
            ErrorType::Errorlex,
        );
        error4.suggestions = vec!["man soga".to_string()];

        // Error 5: {ovddasttit}${verb,conc|ovddastit}
        let error5 = make_simple_error(
            "ovddasttit",
            112,
            112 + "ovddasttit".len(),
            ErrorType::Errorort,
            vec!["ovddastit"],
            Some("verb,conc"),
        );

        let sentence = Sentence::with_errors(
            "heaitit dáhkaluddame ahte sis máhkaš livččii makkarge...".to_string(),
            vec![error1, error2, error3, error4, error5],
        );

        assert_eq!(sentence.error_count(), 5);
        
        // Verify the nested error (error4)
        match &sentence.errors[3].form {
            ErrorContent::Nested(segments) => {
                assert_eq!(segments.len(), 2);
                match &segments[0] {
                    ErrorSegment::Error(inner) => {
                        assert_eq!(inner.errortype, ErrorType::Errorort);
                        assert_eq!(inner.comment, "interr,á");
                    }
                    _ => panic!("Expected Error segment"),
                }
            }
            _ => panic!("Expected nested content in error4"),
        }
    }

    #[test]
    fn test_nested_errorort_and_errorlex_in_errormorphsyn() {
        // Input: {{Bearpmahat}${noun,svow|Bearpmehat} {earuha}€{verb,v,w|sirre}}
        // £{verb,fin,pl3prs,sg3prs,agr|Bearpmehat sirrejit} uskki ja loaiddu.
        
        let inner_error1 = make_simple_error(
            "Bearpmahat",
            0,
            "Bearpmahat".len(),
            ErrorType::Errorort,
            vec!["Bearpmehat"],
            Some("noun,svow"),
        );

        let inner_error2 = make_simple_error(
            "earuha",
            11,
            11 + "earuha".len(),
            ErrorType::Errorlex,
            vec!["sirre"],
            Some("verb,v,w"),
        );

        let mut outer_error = ErrorMarkup::new_nested(
            vec![
                ErrorSegment::Error(Box::new(inner_error1)),
                ErrorSegment::Error(Box::new(inner_error2)),
            ],
            0,
            "Bearpmahat earuha".len(),
            ErrorType::Errormorphsyn,
        );
        outer_error.comment = "verb,fin,pl3prs,sg3prs,agr".to_string();
        outer_error.suggestions = vec!["Bearpmehat sirrejit".to_string()];

        // Verify structure has two nested errors
        match &outer_error.form {
            ErrorContent::Nested(segments) => {
                assert_eq!(segments.len(), 2);
                
                match &segments[0] {
                    ErrorSegment::Error(e) => assert_eq!(e.errortype, ErrorType::Errorort),
                    _ => panic!("Expected Error segment"),
                }
                
                match &segments[1] {
                    ErrorSegment::Error(e) => assert_eq!(e.errortype, ErrorType::Errorlex),
                    _ => panic!("Expected Error segment"),
                }
            }
            _ => panic!("Expected nested content"),
        }
    }

    #[test]
    fn test_nested_errorortreal_in_errorlex() {
        // Input: Mirja ja Line leaba {{gulahallan olbmožat}¢{noun,cmp|gulahallanolbmožat}}€{gulahallanolbmot}
        
        let inner_error = make_simple_error(
            "gulahallan olbmožat",
            22,
            22 + "gulahallan olbmožat".len(),
            ErrorType::Errorortreal,
            vec!["gulahallanolbmožat"],
            Some("noun,cmp"),
        );

        let mut outer_error = ErrorMarkup::new_nested(
            vec![ErrorSegment::Error(Box::new(inner_error))],
            22,
            22 + "gulahallan olbmožat".len(),
            ErrorType::Errorlex,
        );
        outer_error.suggestions = vec!["gulahallanolbmot".to_string()];

        assert_eq!(outer_error.errortype, ErrorType::Errorlex);
        
        match &outer_error.form {
            ErrorContent::Nested(segments) => {
                match &segments[0] {
                    ErrorSegment::Error(inner) => {
                        assert_eq!(inner.errortype, ErrorType::Errorortreal);
                        assert_eq!(inner.comment, "noun,cmp");
                    }
                    _ => panic!("Expected Error segment"),
                }
            }
            _ => panic!("Expected nested content"),
        }
    }

    #[test]
    fn test_triple_nested_errors() {
        // Input: {Ovddit geasis}£{noun,advl,gensg,locsg,case|Ovddit geasi}
        // {{{čoaggen}${verb,mono|čoggen} ollu jokŋat}
        // £{noun,obj,genpl,nompl,case|čoggen ollu joŋaid} ja sarridat}
        // £{noun,obj,genpl,nompl,case|čoggen ollu joŋaid ja sarridiid}
        
        // Error 1: {Ovddit geasis}£{...}
        let error1 = make_simple_error(
            "Ovddit geasis",
            0,
            "Ovddit geasis".len(),
            ErrorType::Errormorphsyn,
            vec!["Ovddit geasi"],
            Some("noun,advl,gensg,locsg,case"),
        );

        // Innermost (level 3): {čoaggen}${verb,mono|čoggen}
        let innermost_error = make_simple_error(
            "čoaggen",
            14,
            14 + "čoaggen".len(),
            ErrorType::Errorort,
            vec!["čoggen"],
            Some("verb,mono"),
        );

        // Middle (level 2): {{čoaggen}${...} ollu jokŋat}£{...}
        let mut middle_error = ErrorMarkup::new_nested(
            vec![
                ErrorSegment::Error(Box::new(innermost_error)),
                ErrorSegment::Text(" ollu jokŋat".to_string()),
            ],
            14,
            14 + "čoaggen ollu jokŋat".len(),
            ErrorType::Errormorphsyn,
        );
        middle_error.comment = "noun,obj,genpl,nompl,case".to_string();
        middle_error.suggestions = vec!["čoggen ollu joŋaid".to_string()];

        // Outermost (level 1): {{{...}£{...} ja sarridat}£{...}
        let mut outer_error = ErrorMarkup::new_nested(
            vec![
                ErrorSegment::Error(Box::new(middle_error)),
                ErrorSegment::Text(" ja sarridat".to_string()),
            ],
            14,
            14 + "čoaggen ollu jokŋat ja sarridat".len(),
            ErrorType::Errormorphsyn,
        );
        outer_error.comment = "noun,obj,genpl,nompl,case".to_string();
        outer_error.suggestions = vec!["čoggen ollu joŋaid ja sarridiid".to_string()];

        let sentence = Sentence::with_errors(
            "Ovddit geasis čoaggen ollu jokŋat ja sarridat".to_string(),
            vec![error1, outer_error],
        );

        assert_eq!(sentence.error_count(), 2);
        
        // Verify triple nesting in the second error
        match &sentence.errors[1].form {
            ErrorContent::Nested(segments) => {
                match &segments[0] {
                    ErrorSegment::Error(middle) => {
                        // Check middle level
                        assert_eq!(middle.errortype, ErrorType::Errormorphsyn);
                        
                        // Check innermost level
                        match &middle.form {
                            ErrorContent::Nested(inner_segments) => {
                                match &inner_segments[0] {
                                    ErrorSegment::Error(innermost) => {
                                        assert_eq!(innermost.errortype, ErrorType::Errorort);
                                        assert_eq!(innermost.comment, "verb,mono");
                                    }
                                    _ => panic!("Expected Error segment at innermost level"),
                                }
                            }
                            _ => panic!("Expected nested content at middle level"),
                        }
                    }
                    _ => panic!("Expected Error segment at outer level"),
                }
            }
            _ => panic!("Expected nested content at outer level"),
        }
    }

    #[test]
    fn test_nested_errorort_inside_errorortreal_epoxy() {
        // Input: Bruk {{epoxi}${noun,cons|epoksy} lim}¢{noun,mix|epoksylim} med god kvalitet.
        
        let inner_error = make_simple_error(
            "epoxi",
            5,
            5 + "epoxi".len(),
            ErrorType::Errorort,
            vec!["epoksy"],
            Some("noun,cons"),
        );

        let mut outer_error = ErrorMarkup::new_nested(
            vec![
                ErrorSegment::Error(Box::new(inner_error)),
                ErrorSegment::Text(" lim".to_string()),
            ],
            5,
            5 + "epoxi lim".len(),
            ErrorType::Errorortreal,
        );
        outer_error.comment = "noun,mix".to_string();
        outer_error.suggestions = vec!["epoksylim".to_string()];

        assert_eq!(outer_error.errortype, ErrorType::Errorortreal);
        assert_eq!(outer_error.comment, "noun,mix");
        assert_eq!(outer_error.suggestions, vec!["epoksylim"]);

        // Verify nested structure
        match &outer_error.form {
            ErrorContent::Nested(segments) => {
                assert_eq!(segments.len(), 2);
                
                match &segments[0] {
                    ErrorSegment::Error(inner) => {
                        assert_eq!(inner.errortype, ErrorType::Errorort);
                        assert_eq!(inner.comment, "noun,cons");
                        assert_eq!(inner.suggestions, vec!["epoksy"]);
                    }
                    _ => panic!("Expected Error segment"),
                }
                
                match &segments[1] {
                    ErrorSegment::Text(text) => {
                        assert_eq!(text, " lim");
                    }
                    _ => panic!("Expected Text segment"),
                }
            }
            _ => panic!("Expected nested content"),
        }
    }
}
