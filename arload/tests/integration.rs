use arload::{
    status::{OutputFormat, OutputHeader, Status, StatusCode},
    transaction::Tag,
    upload_files_stream,
    utils::{TempDir, TempFrom},
    Arweave, Error, Methods as ArewaveMethods,
};
use futures::{future::try_join_all, StreamExt};
use glob::glob;
use std::{iter, path::PathBuf, time::Duration};
use tokio::time::sleep;

async fn get_arweave() -> Result<Arweave, Error> {
    let keypair_path =
        "tests/fixtures/arweave-keyfile-MlV6DeOtRmakDOf6vgOBlif795tcWimgyPsYYNQ8q1Y.json";
    let base_url = "http://localhost:1984/";
    let arweave = Arweave::from_keypair_path(PathBuf::from(keypair_path), Some(base_url)).await?;
    Ok(arweave)
}

async fn mine(arweave: &Arweave) -> Result<(), Error> {
    let url = arweave.base_url.join("mine")?;
    let resp = reqwest::get(url).await?.text().await?;
    // Give the node server a chance
    sleep(Duration::from_secs(1)).await;
    println!("mine resp: {}", resp);
    Ok(())
}

#[tokio::test]
async fn test_post_transaction() -> Result<(), Error> {
    let arweave = get_arweave().await?;
    // Don't run if test server is not running.
    if let Err(_) = reqwest::get(arweave.base_url.join("info")?).await {
        println!("Test server not running.");
        return Ok(());
    }

    let file_path = PathBuf::from("tests/fixtures/0.png");
    let transaction = arweave
        .create_transaction_from_file_path(file_path, None, None, Some(0))
        .await?;

    let signed_transaction = arweave.sign_transaction(transaction)?;
    println!("signed_transaction: {:?}", &signed_transaction);
    arweave.post_transaction(&signed_transaction, None).await?;

    let url = arweave.base_url.join("mine")?;
    let resp = reqwest::get(url).await?.text().await?;
    println!("mine: {}", resp);

    let status = arweave.get_raw_status(&signed_transaction.id).await?;
    println!("{:?}", status);
    Ok(())
}

#[tokio::test]
async fn test_upload_file_from_path() -> Result<(), Error> {
    let arweave = get_arweave().await?;
    // Don't run if test server is not running.
    if let Err(_) = reqwest::get(arweave.base_url.join("info")?).await {
        println!("Test server not running.");
        return Ok(());
    }

    let file_path = PathBuf::from("tests/fixtures/0.png");
    let temp_log_dir = TempDir::from_str("../target/tmp/").await?;
    let log_dir = temp_log_dir.0.clone();

    let status = arweave
        .upload_file_from_path(
            file_path.clone(),
            Some(log_dir.clone()),
            None,
            None,
            Some(0),
        )
        .await?;

    let read_status = arweave.read_status(file_path, log_dir.clone()).await?;
    println!("{:?}", &read_status);
    assert_eq!(status, read_status);
    Ok(())
}

#[tokio::test]
async fn test_update_status() -> Result<(), Error> {
    let arweave = get_arweave().await?;
    // Don't run if test server is not running.
    if let Err(_) = reqwest::get(arweave.base_url.join("info")?).await {
        println!("Test server not running.");
        return Ok(());
    }

    let file_path = PathBuf::from("tests/fixtures/0.png");
    let temp_log_dir = TempDir::from_str("../target/tmp/").await?;
    let log_dir = temp_log_dir.0.clone();

    let _ = arweave
        .upload_file_from_path(
            file_path.clone(),
            Some(log_dir.clone()),
            None,
            None,
            Some(0),
        )
        .await?;

    let read_status = arweave
        .read_status(file_path.clone(), log_dir.clone())
        .await?;
    assert_eq!(read_status.status, StatusCode::Submitted);

    let url = arweave.base_url.join("mine")?;
    let resp = reqwest::get(url).await?.text().await?;
    println!("mine resp: {}", resp);

    let updated_status = arweave.update_status(file_path, log_dir.clone()).await?;
    println!("{:?}", &updated_status);
    assert_eq!(updated_status.status, StatusCode::Confirmed);
    assert!(updated_status.last_modified > read_status.last_modified);
    Ok(())
}

#[tokio::test]
async fn test_upload_files_from_paths_without_tags() -> Result<(), Error> {
    let arweave = get_arweave().await?;
    // Don't run if test server is not running.
    if let Err(_) = reqwest::get(arweave.base_url.join("info")?).await {
        println!("Test server not running.");
        return Ok(());
    }

    let paths_iter = glob("tests/fixtures/*.png")?.filter_map(Result::ok);
    let temp_log_dir = TempDir::from_str("../target/tmp/").await?;
    let log_dir = temp_log_dir.0.clone();

    #[allow(unused_assignments)]
    let mut tags_iter = Some(iter::repeat(Some(Vec::<Tag>::new())));
    tags_iter = None;

    let statuses = arweave
        .upload_files_from_paths(paths_iter, Some(log_dir.clone()), tags_iter, None, Some(0))
        .await?;

    let paths_iter = glob("tests/fixtures/*.png")?.filter_map(Result::ok);
    let read_statuses = arweave.read_statuses(paths_iter, log_dir.clone()).await?;
    assert_eq!(statuses, read_statuses);
    Ok(())
}

#[tokio::test]
async fn test_update_statuses() -> Result<(), Error> {
    let arweave = get_arweave().await?;
    // Don't run if test server is not running.
    if let Err(_) = reqwest::get(arweave.base_url.join("info")?).await {
        println!("Test server not running.");
        return Ok(());
    }

    let paths_iter = glob("tests/fixtures/*.png")?.filter_map(Result::ok);
    let temp_log_dir = TempDir::from_str("../target/tmp/").await?;
    let log_dir = temp_log_dir.0.clone();

    #[allow(unused_assignments)]
    let mut tags_iter = Some(iter::repeat(Some(Vec::<Tag>::new())));
    tags_iter = None;

    let statuses = arweave
        .upload_files_from_paths(paths_iter, Some(log_dir.clone()), tags_iter, None, Some(0))
        .await?;

    println!("{:?}", statuses);
    let url = arweave.base_url.join("mine")?;
    let resp = reqwest::get(url).await?.text().await?;
    println!("mine resp: {}", resp);

    let paths_iter = glob("tests/fixtures/*.png")?.filter_map(Result::ok);

    let update_statuses = arweave.update_statuses(paths_iter, log_dir.clone()).await?;

    println!("{:?}", update_statuses);

    let all_confirmed = update_statuses
        .iter()
        .all(|s| s.status == StatusCode::Confirmed);
    assert!(all_confirmed);
    Ok(())
}

#[tokio::test]
async fn test_filter_statuses() -> Result<(), Error> {
    let arweave = get_arweave().await?;
    // Don't run if test server is not running.
    if let Err(_) = reqwest::get(arweave.base_url.join("info")?).await {
        println!("Test server not running.");
        return Ok(());
    }

    let _ = mine(&arweave).await?;
    let paths_iter = glob("tests/fixtures/[0-4]*.png")?.filter_map(Result::ok);

    let temp_log_dir = TempDir::from_str("../target/tmp/").await?;
    let log_dir = temp_log_dir.0.clone();

    #[allow(unused_assignments)]
    let mut tags_iter = Some(iter::repeat(Some(Vec::<Tag>::new())));
    tags_iter = None;

    // Upload the first five files.
    let _statuses = arweave
        .upload_files_from_paths(
            paths_iter,
            Some(log_dir.clone()),
            tags_iter.clone(),
            None,
            Some(0),
        )
        .await?;

    // Update statuses.
    let paths_iter = glob("tests/fixtures/[0-4]*.png")?.filter_map(Result::ok);
    let update_statuses = arweave.update_statuses(paths_iter, log_dir.clone()).await?;

    println!("{:?}", update_statuses);
    assert_eq!(update_statuses.len(), 5);

    // There should be 5 StatusCode::Pending.
    let paths_iter = glob("tests/fixtures/[0-4].png")?.filter_map(Result::ok);
    let pending = arweave
        .filter_statuses(paths_iter, log_dir.clone(), StatusCode::Pending, None)
        .await?;
    assert_eq!(pending.len(), 5);
    println!("{:?}", pending);

    // Then mine
    let _ = mine(&arweave).await?;

    // Now when we update statuses we should get five confirmed.
    let paths_iter = glob("tests/fixtures/[0-4]*.png")?.filter_map(Result::ok);
    let _updated_statuses = arweave.update_statuses(paths_iter, log_dir.clone()).await?;
    let paths_iter = glob("tests/fixtures/[0-4].png")?.filter_map(Result::ok);
    let confirmed = arweave
        .filter_statuses(paths_iter, log_dir.clone(), StatusCode::Confirmed, None)
        .await?;
    assert_eq!(confirmed.len(), 5);
    println!("{:?}", confirmed);

    // Now write statuses to the log_dir without uploading them so that we get not found when we try
    // to fetch their raw statuses from the server.
    let paths_iter = glob("tests/fixtures/[5-9]*.png")?.filter_map(Result::ok);
    let transactions = try_join_all(
        paths_iter.map(|p| arweave.create_transaction_from_file_path(p, None, None, Some(0))),
    )
    .await?;
    let _ = try_join_all(
        transactions
            .into_iter()
            .map(|t| arweave.sign_transaction(t))
            .filter_map(Result::ok)
            .zip(glob("tests/fixtures/[5-9]*.png")?.filter_map(Result::ok))
            .map(|(s, p)| {
                arweave.write_status(
                    Status {
                        id: s.id.clone(),
                        reward: s.reward,
                        file_path: Some(p),
                        ..Default::default()
                    },
                    log_dir.clone(),
                )
            }),
    )
    .await?;

    // We should now have ten statuses
    let paths_iter = glob("tests/fixtures/[0-9]*.png")?.filter_map(Result::ok);
    let updated_statuses = arweave.update_statuses(paths_iter, log_dir.clone()).await?;
    assert_eq!(updated_statuses.len(), 10);

    // With five not found
    let paths_iter = glob("tests/fixtures/[0-9].png")?.filter_map(Result::ok);
    let not_found = arweave
        .filter_statuses(paths_iter, log_dir.clone(), StatusCode::NotFound, None)
        .await?;
    assert_eq!(not_found.len(), 5);

    // Now if we upload transactions for the not found statuses and mine we should have ten confirmed transactions.
    let paths_iter = glob("tests/fixtures/[5-9]*.png")?.filter_map(Result::ok);
    let _statuses = arweave
        .upload_files_from_paths(paths_iter, Some(log_dir.clone()), tags_iter, None, Some(0))
        .await?;

    let _ = mine(&arweave).await?;

    let paths_iter = glob("tests/fixtures/[0-9]*.png")?.filter_map(Result::ok);
    let updated_statuses = arweave.update_statuses(paths_iter, log_dir.clone()).await?;
    assert_eq!(updated_statuses.len(), 10);

    let paths_iter = glob("tests/fixtures/[0-9].png")?.filter_map(Result::ok);
    let confirmed = arweave
        .filter_statuses(paths_iter, log_dir.clone(), StatusCode::Confirmed, None)
        .await?;
    assert_eq!(confirmed.len(), 10);
    Ok(())
}

#[tokio::test]
async fn test_upload_files_stream() -> Result<(), Error> {
    let arweave = get_arweave().await?;
    // Don't run if test server is not running.
    if let Err(_) = reqwest::get(arweave.base_url.join("info")?).await {
        println!("Test server not running.");
        return Ok(());
    }

    let _ = mine(&arweave).await?;
    let paths_iter = glob("tests/fixtures/[0-9]*.png")?.filter_map(Result::ok);

    let temp_log_dir = TempDir::from_str("../target/tmp/").await?;
    let _log_dir = temp_log_dir.0.clone();

    let mut _tags_iter = Some(iter::repeat(Some(Vec::<Tag>::new())));
    _tags_iter = None;

    let mut stream = upload_files_stream(&arweave, paths_iter, None, None, None, 3);

    let output_format = OutputFormat::JsonCompact;
    println!("{}", Status::header_string(&output_format));
    while let Some(Ok(status)) = stream.next().await {
        print!("{}", output_format.formatted_string(&status));
    }
    Ok(())
}
