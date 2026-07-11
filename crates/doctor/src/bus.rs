//! Wires [`crate::finding::Finding`] onto the `doctor.finding` bus topic
//! (`contracts/bus_events.md`): `finding_id, severity (info/warn/error),
//! what, why, action, fix_command?`. `Finding`'s own shape already matches
//! that payload field for field, so this is a direct [`BusEvent`]
//! implementation rather than a second, parallel struct to keep in sync.

use operant_core::bus::events::BusEvent;

use crate::finding::Finding;

impl BusEvent for Finding {
    const TOPIC: &'static str = "doctor.finding";
}

#[cfg(test)]
mod tests {
    use operant_core::Bus;

    use super::*;
    use crate::catalog::ErrorKind;
    use crate::finding::Severity;

    #[test]
    fn topic_matches_the_contract() {
        assert_eq!(Finding::TOPIC, "doctor.finding");
    }

    #[test]
    fn finding_publishes_and_roundtrips_on_the_bus() {
        let bus = Bus::new();
        let sub = bus.subscribe("doctor.*");

        let finding = Finding::from_catalog(
            "disk_free",
            Severity::Error,
            &ErrorKind::DiskSpaceLow.entry(),
        );
        bus.publish_event(&finding).expect("Finding serializes");

        let env = sub.rx.try_recv().expect("doctor.finding delivered");
        assert_eq!(env.topic, "doctor.finding");
        let back: Finding = serde_json::from_value(env.payload).expect("payload deserializes back");
        assert_eq!(back, finding);
    }

    #[test]
    fn non_doctor_subscribers_do_not_see_the_finding() {
        let bus = Bus::new();
        let run_sub = bus.subscribe("run.*");
        bus.publish_event(&Finding::healthy("audio_devices_present", "ok", "checked"))
            .unwrap();
        assert!(run_sub.rx.try_recv().is_err());
    }
}
