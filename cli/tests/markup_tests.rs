/// Integration tests for error markup parsing
///
/// These tests are based on the Python test suite for converting
/// plaintext markup to XML format. They verify the structure of
/// ErrorMarkup and Sentence types.
#[cfg(test)]
mod markup_tests {
    use divvun_runtime_cli::command::yaml_test::{ErrorMarkup, ErrorType, Sentence, ErrorContent, ErrorSegment};

    /// Test helper: Create a simple error with suggestions and optional comment
    fn make_error(
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
    fn test_errorlang_infinity() {
        // Input: {molekylærbiologimi}∞{kal,bio}
        let error = make_error(
            "molekylærbiologimi",
            0,
            "molekylærbiologimi".len(),
            ErrorType::Errorlang,
            vec!["kal,bio"],
            None,
        );

        assert_eq!(error.errortype, ErrorType::Errorlang);
        assert_eq!(error.suggestions, vec!["kal,bio"]);
    }

    #[test]
    fn test_quote_char() {
        // Input: {"sjievnnijis"}${conc,vnn-vnnj|sjievnnjis}
        let error = make_error(
            "\"sjievnnijis\"",
            0,
            "\"sjievnnijis\"".len(),
            ErrorType::Errorort,
            vec!["sjievnnjis"],
            Some("conc,vnn-vnnj"),
        );

        assert_eq!(error.errortype, ErrorType::Errorort);
        assert_eq!(error.comment, "conc,vnn-vnnj");
        assert_eq!(error.suggestions, vec!["sjievnnjis"]);
    }

    #[test]
    fn test_errorort1() {
        // Input: {jne.}${adv,typo|jna.}
        let error = make_error(
            "jne.",
            0,
            "jne.".len(),
            ErrorType::Errorort,
            vec!["jna."],
            Some("adv,typo"),
        );

        assert_eq!(error.errortype, ErrorType::Errorort);
        assert_eq!(error.comment, "adv,typo");
        assert_eq!(error.suggestions, vec!["jna."]);
    }

    #[test]
    fn test_error_morphsyn1() {
        // Input: {Nieiddat leat nuorra}£{a,spred,nompl,nomsg,agr|Nieiddat leat nuorat}
        let error = make_error(
            "Nieiddat leat nuorra",
            0,
            "Nieiddat leat nuorra".len(),
            ErrorType::Errormorphsyn,
            vec!["Nieiddat leat nuorat"],
            Some("a,spred,nompl,nomsg,agr"),
        );

        assert_eq!(error.errortype, ErrorType::Errormorphsyn);
        assert_eq!(error.comment, "a,spred,nompl,nomsg,agr");
        assert_eq!(error.suggestions, vec!["Nieiddat leat nuorat"]);
    }

    #[test]
    fn test_sentence_with_two_simple_errors() {
        // Input: gitta {Nordkjosbotn'ii}${Nordkjosbotnii} (mii lea ge 
        // {nordkjosbotn}${Nordkjosbotn} sámegillii? Muhtin, veahket mu!) gos
        
        let text = "gitta Nordkjosbotn'ii (mii lea ge nordkjosbotn sámegillii? Muhtin, veahket mu!) gos";
        
        let error1 = make_error(
            "Nordkjosbotn'ii",
            6,
            6 + "Nordkjosbotn'ii".len(),
            ErrorType::Errorort,
            vec!["Nordkjosbotnii"],
            None,
        );

        let error2 = make_error(
            "nordkjosbotn",
            35,
            35 + "nordkjosbotn".len(),
            ErrorType::Errorort,
            vec!["Nordkjosbotn"],
            None,
        );

        let sentence = Sentence::with_errors(
            text.to_string(),
            vec![error1, error2],
        );

        assert_eq!(sentence.error_count(), 2);
        assert_eq!(sentence.errors[0].suggestions, vec!["Nordkjosbotnii"]);
        assert_eq!(sentence.errors[1].suggestions, vec!["Nordkjosbotn"]);
    }

    #[test]
    fn test_only_text_no_errors() {
        // Input: Muittán doložiid
        let sentence = Sentence::new("Muittán doložiid".to_string());

        assert!(!sentence.has_errors());
        assert_eq!(sentence.text, "Muittán doložiid");
    }

    #[test]
    fn test_paragraph_character() {
        // Input: Vuodoláhkaj §110a
        // The § character should not be confused with error markup
        let sentence = Sentence::new("Vuodoláhkaj §110a".to_string());

        assert!(!sentence.has_errors());
        assert_eq!(sentence.text, "Vuodoláhkaj §110a");
    }

    #[test]
    fn test_errorort_with_slash() {
        // Input: {magistter/}${loan,vowlat,e-a|magisttar}
        let error = make_error(
            "magistter/",
            0,
            "magistter/".len(),
            ErrorType::Errorort,
            vec!["magisttar"],
            Some("loan,vowlat,e-a"),
        );

        assert_eq!(error.comment, "loan,vowlat,e-a");
        assert_eq!(error.suggestions, vec!["magisttar"]);
    }

    #[test]
    fn test_error_correct_generic() {
        // Input: {1]}§{Ij}
        let error = make_error(
            "1]",
            0,
            "1]".len(),
            ErrorType::Error,
            vec!["Ij"],
            None,
        );

        assert_eq!(error.errortype, ErrorType::Error);
        assert_eq!(error.suggestions, vec!["Ij"]);
    }

    #[test]
    fn test_error_lex1() {
        // Input: {dábálaš}€{adv,adj,der|dábálaččat}
        let error = make_error(
            "dábálaš",
            0,
            "dábálaš".len(),
            ErrorType::Errorlex,
            vec!["dábálaččat"],
            Some("adv,adj,der"),
        );

        assert_eq!(error.errortype, ErrorType::Errorlex);
        assert_eq!(error.comment, "adv,adj,der");
        assert_eq!(error.suggestions, vec!["dábálaččat"]);
    }

    #[test]
    fn test_error_ortreal1() {
        // Input: {ráhččamušaid}¢{noun,mix|rahčamušaid}
        let error = make_error(
            "ráhččamušaid",
            0,
            "ráhččamušaid".len(),
            ErrorType::Errorortreal,
            vec!["rahčamušaid"],
            Some("noun,mix"),
        );

        assert_eq!(error.errortype, ErrorType::Errorortreal);
        assert_eq!(error.comment, "noun,mix");
        assert_eq!(error.suggestions, vec!["rahčamušaid"]);
    }

    #[test]
    fn test_error_syn1() {
        // Input: {riŋgen nieidda lusa}¥{x,pph|riŋgen niidii}
        let error = make_error(
            "riŋgen nieidda lusa",
            0,
            "riŋgen nieidda lusa".len(),
            ErrorType::Errorsyn,
            vec!["riŋgen niidii"],
            Some("x,pph"),
        );

        assert_eq!(error.errortype, ErrorType::Errorsyn);
        assert_eq!(error.comment, "x,pph");
        assert_eq!(error.suggestions, vec!["riŋgen niidii"]);
    }

    #[test]
    fn test_error_syn_with_empty_correction() {
        // Input: {ovtta}¥{num,redun| }
        let error = make_error(
            "ovtta",
            0,
            "ovtta".len(),
            ErrorType::Errorsyn,
            vec![""],
            Some("num,redun"),
        );

        assert_eq!(error.comment, "num,redun");
        assert_eq!(error.suggestions, vec![""]);
    }

    #[test]
    fn test_nested_error_format() {
        // Input: {{A  B}‰{notspace|A B}  C}‰{notspace|A B C}
        // This represents nested errorformat markups
        
        let inner_error = ErrorMarkup::with_suggestions_and_comment(
            "A  B".to_string(),
            0,
            4,
            ErrorType::Errorformat,
            vec!["A B".to_string()],
            "notspace".to_string(),
        );

        let mut outer_error = ErrorMarkup::new_nested(
            vec![
                ErrorSegment::Error(Box::new(inner_error)),
                ErrorSegment::Text("  C".to_string()),
            ],
            0,
            8,
            ErrorType::Errorformat,
        );
        outer_error.comment = "notspace".to_string();
        outer_error.suggestions = vec!["A B C".to_string()];

        // Verify nested structure
        match &outer_error.form {
            ErrorContent::Nested(segments) => {
                assert_eq!(segments.len(), 2);
                match &segments[0] {
                    ErrorSegment::Error(inner) => {
                        assert_eq!(inner.errortype, ErrorType::Errorformat);
                        assert_eq!(inner.comment, "notspace");
                    }
                    _ => panic!("Expected Error segment"),
                }
            }
            _ => panic!("Expected nested content"),
        }
    }

    #[test]
    fn test_sentence_with_multiple_errors_different_types() {
        // Input: ( {nissonin}¢{noun,suf|nissoniin} dušše {0.6 %:s}£{0.6 %} )
        
        let error1 = make_error(
            "nissonin",
            2,
            2 + "nissonin".len(),
            ErrorType::Errorortreal,
            vec!["nissoniin"],
            Some("noun,suf"),
        );

        let error2 = make_error(
            "0.6 %:s",
            2 + "nissonin".len() + 7, // approximate position
            2 + "nissonin".len() + 7 + "0.6 %:s".len(),
            ErrorType::Errormorphsyn,
            vec!["0.6 %"],
            None,
        );

        let sentence = Sentence::with_errors(
            "( nissonin dušše 0.6 %:s )".to_string(),
            vec![error1, error2],
        );

        assert_eq!(sentence.error_count(), 2);
        assert_eq!(sentence.errors[0].errortype, ErrorType::Errorortreal);
        assert_eq!(sentence.errors[1].errortype, ErrorType::Errormorphsyn);
    }

    #[test]
    fn test_inline_multiple_corrections() {
        // Input: {leimme}£{leimmet///leat}
        // This should be represented as multiple suggestions
        
        let mut error = ErrorMarkup::new(
            "leimme".to_string(),
            0,
            "leimme".len(),
            ErrorType::Errormorphsyn,
        );
        error.suggestions = vec!["leimmet".to_string(), "leat".to_string()];

        assert_eq!(error.errortype, ErrorType::Errormorphsyn);
        assert_eq!(error.suggestions.len(), 2);
        assert_eq!(error.suggestions, vec!["leimmet", "leat"]);
    }

    #[test]
    fn test_multiple_errors_in_sentence() {
        // Test with 3 errors in same sentence
        let error1 = make_error("njiŋŋalas", 15, 15 + "njiŋŋalas".len(), 
                                ErrorType::Errorort, vec!["njiŋŋálas"], Some("noun,á"));
        let error2 = make_error("ságahuvvon", 25, 25 + "ságahuvvon".len(),
                                ErrorType::Errorort, vec!["sagahuvvon"], Some("verb,a"));
        let error3 = make_error("guovža-klána", 40, 40 + "guovža-klána".len(),
                                ErrorType::Errorort, vec!["guovžaklána"], Some("noun,cmp"));

        let sentence = Sentence::with_errors(
            "(haploida) ja njiŋŋalas ságahuvvon manneseallas guovža-klána".to_string(),
            vec![error1, error2, error3],
        );

        assert_eq!(sentence.error_count(), 3);
        assert_eq!(sentence.errors[0].comment, "noun,á");
        assert_eq!(sentence.errors[1].comment, "verb,a");
        assert_eq!(sentence.errors[2].comment, "noun,cmp");
    }

    #[test]
    fn test_json_roundtrip_with_errors() {
        let error = make_error(
            "čohke",
            0,
            6,
            ErrorType::Errorortreal,
            vec!["čohkke"],
            Some("test"),
        );

        let sentence = Sentence::with_errors(
            "čohke text".to_string(),
            vec![error],
        );

        let json = serde_json::to_string_pretty(&sentence).unwrap();
        let deserialized: Sentence = serde_json::from_str(&json).unwrap();

        assert_eq!(sentence, deserialized);
    }
}
