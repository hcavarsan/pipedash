use tempfile::TempDir;

use super::{
    LocalStorage,
    StorageBackend,
};

#[tokio::test]
async fn test_local_storage() {
    let temp_dir = TempDir::new().unwrap();
    let storage = LocalStorage::new(temp_dir.path().to_path_buf());

    let data = b"Hello, World!";
    let meta = storage.put("test.txt", data, None).await.unwrap();
    assert_eq!(meta.key, "test.txt");
    assert_eq!(meta.size, data.len() as u64);

    let retrieved = storage.get("test.txt").await.unwrap();
    assert_eq!(retrieved, data);

    let list = storage.list(None).await.unwrap();
    assert_eq!(list.len(), 1);
}
