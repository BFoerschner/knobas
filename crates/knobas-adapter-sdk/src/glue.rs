//! The generated `Guest` traits, implemented once for every [`Adapter`]:
//! each WIT function decodes what the host sent, calls the method of the
//! same name, and encodes its answer.

use crate::bindings::exports::knobas::adapter::source::{Guest, GuestAdapter};
use crate::bindings::knobas::adapter::types as wit;
use crate::convert::{self, error_to_wit, from_json, to_json};
use crate::{Adapter, Sink};

/// The `adapter` resource: one [`Adapter`] for one call.
#[doc(hidden)]
pub struct Component<A>(A);

impl<A: Adapter> Guest for A {
    type Adapter = Component<A>;
}

/// `Option<T>` as the `option<json>` the WIT answers with.
fn json_option<T: serde::Serialize>(
    answer: Result<Option<T>, knobas_source::SourceError>,
) -> Result<Option<String>, wit::SourceError> {
    answer
        .map(|value| value.as_ref().map(to_json))
        .map_err(error_to_wit)
}

impl<A: Adapter> GuestAdapter for Component<A> {
    fn new(instance: wit::Instance) -> Self {
        Self(A::new(convert::instance(instance)))
    }

    fn descriptor(&self) -> String {
        to_json(&self.0.descriptor())
    }

    fn test_connection(&self) -> Result<wit::ConnectionInfo, wit::SourceError> {
        self.0
            .test_connection()
            .map(convert::connection_info)
            .map_err(error_to_wit)
    }

    fn sync(&self, cursor: Option<String>) -> Result<String, wit::SourceError> {
        self.0
            .sync(cursor, &mut Sink { _host: () })
            .map_err(error_to_wit)
    }

    fn write(&self, op: String) -> Result<wit::WriteReceipt, wit::SourceError> {
        let op = from_json(&op, "write op").map_err(error_to_wit)?;
        self.0.write(op).map(convert::receipt).map_err(error_to_wit)
    }

    fn reachable_transitions(&self, entity: String) -> Result<Vec<String>, wit::SourceError> {
        self.0.reachable_transitions(&entity).map_err(error_to_wit)
    }

    fn view(
        &self,
        entity: String,
        section: Option<String>,
    ) -> Result<Option<String>, wit::SourceError> {
        json_option(self.0.view(&entity, section.as_deref()))
    }

    fn pipeline_settings(&self, entity: String) -> Result<Option<String>, wit::SourceError> {
        json_option(self.0.pipeline_settings(&entity))
    }

    fn older_runs(
        &self,
        entity: String,
        before: String,
        count: u32,
    ) -> Result<Option<Vec<wit::SyncItem>>, wit::SourceError> {
        self.0
            .older_runs(&entity, &before, count)
            .map(|runs| runs.map(|runs| runs.into_iter().map(convert::item).collect()))
            .map_err(error_to_wit)
    }

    fn run_chain(&self, run: String) -> Result<Option<Vec<wit::RunChainStep>>, wit::SourceError> {
        self.0
            .run_chain(&run)
            .map(|steps| {
                steps.map(|steps| steps.into_iter().map(convert::run_chain_step).collect())
            })
            .map_err(error_to_wit)
    }

    fn run_log(
        &self,
        entity: String,
        from: Option<u64>,
        count: u32,
    ) -> Result<Option<String>, wit::SourceError> {
        json_option(self.0.run_log(&entity, from, count))
    }

    fn run_tests(&self, entity: String) -> Result<Option<String>, wit::SourceError> {
        json_option(self.0.run_tests(&entity))
    }

    fn test_history(
        &self,
        entity: String,
        test: String,
    ) -> Result<Option<String>, wit::SourceError> {
        json_option(self.0.test_history(&entity, &test))
    }

    fn run_artifacts(
        &self,
        entity: String,
        path: String,
    ) -> Result<Option<String>, wit::SourceError> {
        json_option(self.0.run_artifacts(&entity, &path))
    }

    fn artifact_preview(
        &self,
        entity: String,
        path: String,
    ) -> Result<Option<String>, wit::SourceError> {
        json_option(self.0.artifact_preview(&entity, &path))
    }
}

/// `query` (1.2.0): a free function, so it constructs its own instance.
#[cfg(not(feature = "v1_0"))]
impl<A: Adapter> crate::bindings::exports::knobas::adapter::query::Guest for A {
    fn translate_query(
        instance: wit::Instance,
        saved_view: String,
    ) -> Result<Option<String>, wit::SourceError> {
        let view: serde_json::Value = from_json(&saved_view, "saved view").map_err(error_to_wit)?;
        json_option(A::new(convert::instance(instance)).translate_query(&view))
    }
}

/// `downloads` (1.1.0): a free function, as `query` is.
#[cfg(not(feature = "v1_0"))]
impl<A: Adapter> crate::bindings::exports::knobas::adapter::downloads::Guest for A {
    fn download(
        instance: wit::Instance,
        entity: String,
        target: String,
    ) -> Result<bool, wit::SourceError> {
        let target = from_json(&target, "download target").map_err(error_to_wit)?;
        A::new(convert::instance(instance))
            .download(&entity, &target, &mut crate::DownloadSink { _host: () })
            .map_err(error_to_wit)
    }
}
