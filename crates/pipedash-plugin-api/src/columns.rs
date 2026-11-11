use crate::schema::{
    CellRenderer,
    ColumnDataType,
    ColumnDefinition,
    ColumnSort,
    ColumnVisibility,
    TableDefinition,
};

impl ColumnDefinition {
    pub fn new(
        id: &str, label: &str, field_path: &str, data_type: ColumnDataType, renderer: CellRenderer,
    ) -> Self {
        Self {
            id: id.to_string(),
            label: label.to_string(),
            description: None,
            field_path: field_path.to_string(),
            data_type,
            renderer,
            visibility: ColumnVisibility::Always,
            default_visible: true,
            width: None,
            sort: ColumnSort::None,
            filterable: false,
        }
    }

    pub fn badge(id: &str, label: &str, field_path: &str) -> Self {
        Self::new(
            id,
            label,
            field_path,
            ColumnDataType::String,
            CellRenderer::Badge,
        )
    }

    pub fn text(id: &str, label: &str, field_path: &str) -> Self {
        Self::new(
            id,
            label,
            field_path,
            ColumnDataType::String,
            CellRenderer::Text,
        )
    }

    pub fn metadata(
        id: &str, label: &str, description: &str, renderer: CellRenderer, default_visible: bool,
    ) -> Self {
        let mut column = Self::new(
            id,
            label,
            &format!("metadata.{id}"),
            ColumnDataType::String,
            renderer,
        )
        .described(description)
        .with_width(140)
        .sortable();
        column.default_visible = default_visible;
        column
    }

    pub fn described(mut self, description: &str) -> Self {
        self.description = Some(description.to_string());
        self
    }

    pub fn with_width(mut self, width: u32) -> Self {
        self.width = Some(width);
        self
    }

    pub fn when_present(mut self) -> Self {
        self.visibility = ColumnVisibility::WhenPresent;
        self
    }

    pub fn hidden(mut self) -> Self {
        self.default_visible = false;
        self
    }

    pub fn sortable(mut self) -> Self {
        self.sort = ColumnSort::Value;
        self
    }

    pub fn history_order(mut self) -> Self {
        self.sort = ColumnSort::HistoryOrder;
        self
    }

    pub fn filterable(mut self) -> Self {
        self.filterable = true;
        self
    }
}

pub fn trigger_event_column(description: &str) -> ColumnDefinition {
    ColumnDefinition::badge("event", "Trigger", "metadata.event")
        .described(description)
        .with_width(130)
        .sortable()
}

impl TableDefinition {
    pub fn new(id: &str, name: &str, description: &str, columns: Vec<ColumnDefinition>) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            description: Some(description.to_string()),
            columns,
        }
    }

    pub fn column_mut(&mut self, id: &str) -> Option<&mut ColumnDefinition> {
        self.columns.iter_mut().find(|column| column.id == id)
    }

    pub fn insert_after(
        &mut self, anchor_id: &str, columns: impl IntoIterator<Item = ColumnDefinition>,
    ) {
        let at = self
            .columns
            .iter()
            .position(|column| column.id == anchor_id)
            .map_or(self.columns.len(), |index| index + 1);
        self.columns.splice(at..at, columns);
    }
}

#[cfg(test)]
mod tests;
