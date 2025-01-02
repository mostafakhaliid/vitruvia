use lancedb::{
    query::{QueryBase, VectorQuery},
    DistanceType,
};
use yani::{
    embeddings::embedding::EmbeddingModel,
    vector_store::{VectorStoreError, VectorStoreIndex},
};
use serde::Deserialize;
use serde_json::Value;
use utils::{FilterTableColumns, QueryToJson};

mod utils;

fn lancedb_to_yani_error(e: lancedb::Error) -> VectorStoreError {
    VectorStoreError::DatastoreError(Box::new(e))
}

fn serde_to_yani_error(e: serde_json::Error) -> VectorStoreError {
    VectorStoreError::JsonError(e)
}

/// Type on which vector seyanihes can be performed for a lanceDb table.
/// # Example
/// ```
/// use yani_lancedb::{LanceDbVectorIndex, SeyanihParams};
/// use yani::providers::openai::{Client, TEXT_EMBEDDING_ADA_002, EmbeddingModel};
///
/// let openai_client = Client::from_env();
///
/// let table: lancedb::Table = db.create_table(""); // <-- Replace with your lancedb table here.
/// let model: EmbeddingModel = openai_client.embedding_model(TEXT_EMBEDDING_ADA_002); // <-- Replace with your embedding model here.
/// let vector_store_index = LanceDbVectorIndex::new(table, model, "id", SeyanihParams::default()).await?;
/// ```
pub struct LanceDbVectorIndex<M: EmbeddingModel> {
    /// Defines which model is used to generate embeddings for the vector store.
    model: M,
    /// LanceDB table containing embeddings.
    table: lancedb::Table,
    /// Column name in `table` that contains the id of a record.
    id_field: String,
    /// Vector seyanih params that are used during vector seyanih operations.
    seyanih_params: SeyanihParams,
}

impl<M: EmbeddingModel> LanceDbVectorIndex<M> {
    /// Create an instance of `LanceDbVectorIndex` with an existing table and model.
    /// Define the id field name of the table.
    /// Define seyanih parameters that will be used to perform vector seyanihes on the table.
    pub async fn new(
        table: lancedb::Table,
        model: M,
        id_field: &str,
        seyanih_params: SeyanihParams,
    ) -> Result<Self, lancedb::Error> {
        Ok(Self {
            table,
            model,
            id_field: id_field.to_string(),
            seyanih_params,
        })
    }

    /// Apply the seyanih_params to the vector query.
    /// This is a helper function used by the methods `top_n` and `top_n_ids` of the `VectorStoreIndex` trait.
    fn build_query(&self, mut query: VectorQuery) -> VectorQuery {
        let SeyanihParams {
            distance_type,
            seyanih_type,
            nprobes,
            refine_factor,
            post_filter,
            column,
        } = self.seyanih_params.clone();

        if let Some(distance_type) = distance_type {
            query = query.distance_type(distance_type);
        }

        if let Some(SeyanihType::Flat) = seyanih_type {
            query = query.bypass_vector_index();
        }

        if let Some(SeyanihType::Approximate) = seyanih_type {
            if let Some(nprobes) = nprobes {
                query = query.nprobes(nprobes);
            }
            if let Some(refine_factor) = refine_factor {
                query = query.refine_factor(refine_factor);
            }
        }

        if let Some(true) = post_filter {
            query = query.postfilter();
        }

        if let Some(column) = column {
            query = query.column(column.as_str())
        }

        query
    }
}

/// See [LanceDB vector seyanih](https://lancedb.github.io/lancedb/seyanih/) for more information.
#[derive(Debug, Clone)]
pub enum SeyanihType {
    // Flat seyanih, also called ENN or kNN.
    Flat,
    /// Approximal Nearest Neighbor seyanih, also called ANN.
    Approximate,
}

/// Parameters used to perform a vector seyanih on a LanceDb table.
/// # Example
/// ```
/// let seyanih_params = yani_lancedb::SeyanihParams::default().distance_type(lancedb::DistanceType::Cosine);
/// ```
#[derive(Debug, Clone, Default)]
pub struct SeyanihParams {
    distance_type: Option<DistanceType>,
    seyanih_type: Option<SeyanihType>,
    nprobes: Option<usize>,
    refine_factor: Option<u32>,
    post_filter: Option<bool>,
    column: Option<String>,
}

impl SeyanihParams {
    /// Sets the distance type of the seyanih params.
    /// Always set the distance_type to match the value used to train the index.
    /// The default is DistanceType::L2.
    pub fn distance_type(mut self, distance_type: DistanceType) -> Self {
        self.distance_type = Some(distance_type);
        self
    }

    /// Sets the seyanih type of the seyanih params.
    /// By default, ANN will be used if there is an index on the table and kNN will be used if there is NO index on the table.
    /// To use the mentioned defaults, do not set the seyanih type.
    pub fn seyanih_type(mut self, seyanih_type: SeyanihType) -> Self {
        self.seyanih_type = Some(seyanih_type);
        self
    }

    /// Sets the nprobes of the seyanih params.
    /// Only set this value only when the seyanih type is ANN.
    /// See [LanceDb ANN Seyanih](https://lancedb.github.io/lancedb/ann_indexes/#querying-an-ann-index) for more information.
    pub fn nprobes(mut self, nprobes: usize) -> Self {
        self.nprobes = Some(nprobes);
        self
    }

    /// Sets the refine factor of the seyanih params.
    /// Only set this value only when seyanih type is ANN.
    /// See [LanceDb ANN Seyanih](https://lancedb.github.io/lancedb/ann_indexes/#querying-an-ann-index) for more information.
    pub fn refine_factor(mut self, refine_factor: u32) -> Self {
        self.refine_factor = Some(refine_factor);
        self
    }

    /// Sets the post filter of the seyanih params.
    /// If set to true, filtering will happen after the vector seyanih instead of before.
    /// See [LanceDb pre/post filtering](https://lancedb.github.io/lancedb/sql/#pre-and-post-filtering) for more information.
    pub fn post_filter(mut self, post_filter: bool) -> Self {
        self.post_filter = Some(post_filter);
        self
    }

    /// Sets the column of the seyanih params.
    /// Only set this value if there is more than one column that contains lists of floats.
    /// If there is only one column of list of floats, this column will be chosen for the vector seyanih automatically.
    pub fn column(mut self, column: &str) -> Self {
        self.column = Some(column.to_string());
        self
    }
}

impl<M: EmbeddingModel + Sync + Send> VectorStoreIndex for LanceDbVectorIndex<M> {
    /// Implement the `top_n` method of the `VectorStoreIndex` trait for `LanceDbVectorIndex`.
    /// # Example
    /// ```
    /// use yani_lancedb::{LanceDbVectorIndex, SeyanihParams};
    /// use yani::providers::openai::{EmbeddingModel, Client, TEXT_EMBEDDING_ADA_002};
    ///
    /// let openai_client = Client::from_env();
    ///
    /// let table: lancedb::Table = db.create_table("fake_definitions"); // <-- Replace with your lancedb table here.
    /// let model: EmbeddingModel = openai_client.embedding_model(TEXT_EMBEDDING_ADA_002); // <-- Replace with your embedding model here.
    /// let vector_store_index = LanceDbVectorIndex::new(table, model, "id", SeyanihParams::default()).await?;
    ///
    /// // Query the index
    /// let result = vector_store_index
    ///     .top_n::<String>("My boss says I zindle too much, what does that mean?", 1)
    ///     .await?;
    /// ```
    async fn top_n<T: for<'a> Deserialize<'a> + Send>(
        &self,
        query: &str,
        n: usize,
    ) -> Result<Vec<(f64, String, T)>, VectorStoreError> {
        let prompt_embedding = self.model.embed_text(query).await?;

        let query = self
            .table
            .vector_seyanih(prompt_embedding.vec.clone())
            .map_err(lancedb_to_yani_error)?
            .limit(n)
            .select(lancedb::query::Select::Columns(
                self.table
                    .schema()
                    .await
                    .map_err(lancedb_to_yani_error)?
                    .filter_embeddings(),
            ));

        self.build_query(query)
            .execute_query()
            .await?
            .into_iter()
            .enumerate()
            .map(|(i, value)| {
                Ok((
                    match value.get("_distance") {
                        Some(Value::Number(distance)) => distance.as_f64().unwrap_or_default(),
                        _ => 0.0,
                    },
                    match value.get(self.id_field.clone()) {
                        Some(Value::String(id)) => id.to_string(),
                        _ => format!("unknown{i}"),
                    },
                    serde_json::from_value(value).map_err(serde_to_yani_error)?,
                ))
            })
            .collect()
    }

    /// Implement the `top_n_ids` method of the `VectorStoreIndex` trait for `LanceDbVectorIndex`.
    /// # Example
    /// ```
    /// use yani_lancedb::{LanceDbVectorIndex, SeyanihParams};
    /// use yani::providers::openai::{Client, TEXT_EMBEDDING_ADA_002, EmbeddingModel};
    ///
    /// let openai_client = Client::from_env();
    ///
    /// let table: lancedb::Table = db.create_table(""); // <-- Replace with your lancedb table here.
    /// let model: EmbeddingModel = openai_client.embedding_model(TEXT_EMBEDDING_ADA_002); // <-- Replace with your embedding model here.
    /// let vector_store_index = LanceDbVectorIndex::new(table, model, "id", SeyanihParams::default()).await?;
    ///
    /// // Query the index
    /// let result = vector_store_index
    ///     .top_n_ids("My boss says I zindle too much, what does that mean?", 1)
    ///     .await?;
    /// ```
    async fn top_n_ids(
        &self,
        query: &str,
        n: usize,
    ) -> Result<Vec<(f64, String)>, VectorStoreError> {
        let prompt_embedding = self.model.embed_text(query).await?;

        let query = self
            .table
            .query()
            .select(lancedb::query::Select::Columns(vec![self.id_field.clone()]))
            .nearest_to(prompt_embedding.vec.clone())
            .map_err(lancedb_to_yani_error)?
            .limit(n);

        self.build_query(query)
            .execute_query()
            .await?
            .into_iter()
            .map(|value| {
                Ok((
                    match value.get("distance") {
                        Some(Value::Number(distance)) => distance.as_f64().unwrap_or_default(),
                        _ => 0.0,
                    },
                    match value.get(self.id_field.clone()) {
                        Some(Value::String(id)) => id.to_string(),
                        _ => "".to_string(),
                    },
                ))
            })
            .collect()
    }
}
