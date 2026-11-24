//! Adopter corpus checks: regression preservation plus one target module per
//! realistic case. Run with `cargo test --test checks <filter>`.

use pinned_logger::format;
use pinned_logger::level;
use pinned_logger::sink::Sink;
use pinned_logger::sink::Format;

mod regression {
    use super::*;

    #[test]
    fn text_rendering_is_stable() {
        assert_eq!(format::text_line(0, 'I', "boot"), "[0000-00:00:00] I boot");
        assert_eq!(
            format::text_line(90061, 'W', "slow"),
            "[0001-01:01:01] W slow"
        );
    }

    #[test]
    fn json_round_trip_escapes_quotes() {
        assert_eq!(
            format::json_line(0, 'I', "say \"hi\""),
            r#"{"time":"0000-00:00:00","level":"I","message":"say \"hi\""}"#
        );
    }

    #[test]
    fn level_filtering_thresholds() {
        assert!(level::is_allowed('I', 'I'));
        assert!(level::is_allowed('I', 'E'));
        assert!(!level::is_allowed('I', 'D'));
        assert!(!level::is_allowed('W', 'I'));
    }

    #[test]
    fn sink_emit_and_drain_round_trip() {
        let mut sink = Sink::new(Format::Text);
        sink.emit("first".to_string());
        sink.emit("second".to_string());
        assert_eq!(sink.drain(), vec!["first".to_string(), "second".to_string()]);
        assert!(sink.drain().is_empty());
    }

    #[test]
    fn render_emits_text_lines_into_sink() {
        let mut sink = Sink::new(Format::Text);
        pinned_logger::render(&mut sink, 0, 'I', "boot");
        assert_eq!(sink.drain(), vec!["[0000-00:00:00] I boot".to_string()]);
    }
}

mod target_multifile_bugfix {
    use super::*;

    #[test]
    fn sink_record_routes_json_when_configured() {
        let mut sink = Sink::new(Format::Json);
        sink.record(0, 'I', "boot");
        assert_eq!(
            sink.drain(),
            vec![r#"{"time":"0000-00:00:00","level":"I","message":"boot"}"#]
        );
    }

    #[test]
    fn render_delegates_to_sink_record() {
        let mut text = Sink::new(Format::Text);
        let mut json = Sink::new(Format::Json);
        pinned_logger::render(&mut text, 1, 'D', "dbg");
        pinned_logger::render(&mut json, 1, 'D', "dbg");
        assert_eq!(text.drain(), vec!["[0000-00:00:01] D dbg".to_string()]);
        assert_eq!(
            json.drain(),
            vec![r#"{"time":"0000-00:00:01","level":"D","message":"dbg"}"#]
        );
    }
}

mod target_edit_after_green {
    use super::*;

    #[test]
    fn timestamp_matrix_is_byte_exact() {
        for (seconds, expected) in [
            (0, "0000-00:00:00"),
            (59, "0000-00:00:59"),
            (60, "0000-00:01:00"),
            (86399, "0000-23:59:59"),
            (86400, "0001-00:00:00"),
            (90061, "0001-01:01:01"),
            (31536000, "0365-00:00:00"),
        ] {
            assert_eq!(format::timestamp(seconds), expected, "at {seconds}s");
        }
    }

    #[test]
    fn timestamp_is_pure_and_repeatable() {
        let first = format::timestamp(123456);
        let second = format::timestamp(123456);
        assert_eq!(first, second);
    }
}

mod target_policy_invalidation {
    use super::*;

    #[test]
    fn policy_cleanup_preserves_logging_behavior() {
        assert!(level::is_allowed('E', 'E'));
        let mut sink = Sink::new(Format::Text);
        sink.emit("[0000-00:00:00] E gone".to_string());
        assert_eq!(
            sink.drain(),
            vec!["[0000-00:00:00] E gone".to_string()]
        );
    }
}

mod target_resume {
    use super::*;

    #[test]
    fn sink_flushes_oldest_lines_when_capacity_reached() {
        let mut sink = Sink::new(Format::Text);
        let mut flushed = Vec::new();
        for i in 0..300 {
            // `record` returns the drained overflow (flushed) lines for the
            // archive once capacity is exceeded.
            for line in sink.record(0, 'I', "x") {
                flushed.push(line);
            }
            let _ = i;
        }
        assert!(!flushed.is_empty(), "overflow must be flushed");
        let remaining = sink.drain();
        assert!(remaining.len() <= 256, "sink stays within capacity");
    }
}
