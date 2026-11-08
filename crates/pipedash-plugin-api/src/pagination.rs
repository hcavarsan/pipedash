use crate::error::{
    PluginError,
    PluginResult,
};
use crate::types::{
    PaginatedResponse,
    PaginationParams,
};

pub fn paginate<T>(items: Vec<T>, params: PaginationParams) -> PluginResult<PaginatedResponse<T>> {
    params.validate().map_err(PluginError::InvalidConfig)?;
    let offset = params
        .calculate_offset()
        .map_err(PluginError::InvalidConfig)?;
    let total_count = items.len();
    let items: Vec<T> = items
        .into_iter()
        .skip(offset)
        .take(params.page_size)
        .collect();
    let has_more = offset.saturating_add(items.len()) < total_count;
    let total_pages = total_count.div_ceil(params.page_size);

    Ok(PaginatedResponse {
        items,
        page: params.page,
        page_size: params.page_size,
        total_count,
        total_pages,
        has_more,
    })
}

#[cfg(test)]
mod tests;
