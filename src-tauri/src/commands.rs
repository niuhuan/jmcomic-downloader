use std::sync::atomic::AtomicI64;
use std::sync::Arc;

// TODO: 用`#![allow(clippy::used_underscore_binding)]`来消除警告
use anyhow::{anyhow, Context};
use parking_lot::{Mutex, RwLock};
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;
use tauri_specta::Event;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use crate::config::Config;
use crate::download_manager::DownloadManager;
use crate::errors::{CommandError, CommandResult};
use crate::events::UpdateDownloadedFavoriteComicEvent;
use crate::extensions::AnyhowErrorToStringChain;
use crate::jm_client::JmClient;
use crate::responses::GetUserProfileRespData;
use crate::types::{Comic, FavoriteSort, GetFavoriteResult, SearchResultVariant, SearchSort};
use crate::{export, logger};

#[tauri::command]
#[specta::specta]
pub fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn get_config(config: State<RwLock<Config>>) -> Config {
    config.read().clone()
}

#[tauri::command(async)]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn save_config(
    app: AppHandle,
    jm_client: State<JmClient>,
    config_state: State<RwLock<Config>>,
    config: Config,
) -> CommandResult<()> {
    let proxy_changed = {
        let config_state = config_state.read();
        config_state.proxy_mode != config.proxy_mode
            || config_state.proxy_host != config.proxy_host
            || config_state.proxy_port != config.proxy_port
    };

    let enable_file_logger = config.enable_file_logger;
    let file_logger_changed = config_state.read().enable_file_logger != enable_file_logger;

    {
        let mut config_state = config_state.write();
        *config_state = config;
        config_state
            .save(&app)
            .map_err(|err| CommandError::from("保存配置失败", err))?;
        tracing::debug!("保存配置成功");
    }

    if proxy_changed {
        jm_client.reload_client();
    }

    if file_logger_changed {
        if enable_file_logger {
            logger::reload_file_logger()
                .map_err(|err| CommandError::from("重新加载文件日志失败", err))?;
        } else {
            logger::disable_file_logger()
                .map_err(|err| CommandError::from("禁用文件日志失败", err))?;
        }
    }

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn login(
    jm_client: State<'_, JmClient>,
    username: String,
    password: String,
) -> CommandResult<GetUserProfileRespData> {
    let user_profile = jm_client
        .login(&username, &password)
        .await
        .map_err(|err| CommandError::from("登录失败", err))?;
    Ok(user_profile)
}

#[tauri::command]
#[specta::specta]
pub async fn get_user_profile(
    jm_client: State<'_, JmClient>,
) -> CommandResult<GetUserProfileRespData> {
    let user_profile = jm_client
        .get_user_profile()
        .await
        .map_err(|err| CommandError::from("获取用户信息失败", err))?;
    Ok(user_profile)
}

#[tauri::command]
#[specta::specta]
pub async fn search(
    app: AppHandle,
    jm_client: State<'_, JmClient>,
    keyword: String,
    page: i64,
    sort: SearchSort,
) -> CommandResult<SearchResultVariant> {
    let search_resp = jm_client
        .search(&keyword, page, sort)
        .await
        .map_err(|err| CommandError::from("搜索失败", err))?;
    let search_result = SearchResultVariant::from_search_resp(&app, search_resp)
        .map_err(|err| CommandError::from("搜索失败", err))?;
    Ok(search_result)
}

#[tauri::command]
#[specta::specta]
pub async fn get_comic(
    app: AppHandle,
    jm_client: State<'_, JmClient>,
    aid: i64,
) -> CommandResult<Comic> {
    let comic_resp_data = jm_client
        .get_comic(aid)
        .await
        .map_err(|err| CommandError::from("获取漫画信息失败", err))?;
    let comic = Comic::from_comic_resp_data(&app, comic_resp_data);
    Ok(comic)
}

#[tauri::command(async)]
#[specta::specta]
pub async fn get_favorite_folder(
    app: AppHandle,
    jm_client: State<'_, JmClient>,
    folder_id: i64,
    page: i64,
    sort: FavoriteSort,
) -> CommandResult<GetFavoriteResult> {
    let get_favorite_resp_data = jm_client
        .get_favorite_folder(folder_id, page, sort)
        .await
        .map_err(|err| CommandError::from("获取收藏夹失败", err))?;
    let get_favorite_result = GetFavoriteResult::from_resp_data(&app, get_favorite_resp_data)
        .map_err(|err| CommandError::from("获取收藏夹失败", err))?;
    Ok(get_favorite_result)
}

#[allow(clippy::needless_pass_by_value)]
#[tauri::command(async)]
#[specta::specta]
pub fn create_download_task(
    download_manager: State<DownloadManager>,
    comic: Comic,
    chapter_id: i64,
) -> CommandResult<()> {
    let comic_title = comic.name.clone();
    download_manager
        .create_download_task(comic, chapter_id)
        .map_err(|err| {
            let err_title = format!("`{comic_title}`的章节ID为`{chapter_id}`的下载任务创建失败");
            CommandError::from(&err_title, err)
        })?;
    tracing::debug!("创建章节ID为`{chapter_id}`的下载任务成功");
    Ok(())
}

#[allow(clippy::needless_pass_by_value)]
#[tauri::command(async)]
#[specta::specta]
pub fn pause_download_task(
    download_manager: State<DownloadManager>,
    chapter_id: i64,
) -> CommandResult<()> {
    download_manager
        .pause_download_task(chapter_id)
        .map_err(|err| {
            CommandError::from(&format!("暂停章节ID为`{chapter_id}`的下载任务失败"), err)
        })?;
    tracing::debug!("暂停章节ID为`{chapter_id}`的下载任务成功");
    Ok(())
}

#[allow(clippy::needless_pass_by_value)]
#[tauri::command(async)]
#[specta::specta]
pub fn resume_download_task(
    download_manager: State<DownloadManager>,
    chapter_id: i64,
) -> CommandResult<()> {
    download_manager
        .resume_download_task(chapter_id)
        .map_err(|err| {
            CommandError::from(&format!("恢复章节ID为`{chapter_id}`的下载任务失败"), err)
        })?;
    tracing::debug!("恢复章节ID为`{chapter_id}`的下载任务成功");
    Ok(())
}

#[allow(clippy::needless_pass_by_value)]
#[tauri::command(async)]
#[specta::specta]
pub fn cancel_download_task(
    download_manager: State<DownloadManager>,
    chapter_id: i64,
) -> CommandResult<()> {
    download_manager
        .cancel_download_task(chapter_id)
        .map_err(|err| {
            CommandError::from(&format!("取消章节ID为`{chapter_id}`的下载任务失败"), err)
        })?;
    tracing::debug!("取消章节ID为`{chapter_id}`的下载任务成功");
    Ok(())
}

#[tauri::command(async)]
#[specta::specta]
pub async fn download_comic(
    app: AppHandle,
    jm_client: State<'_, JmClient>,
    download_manager: State<'_, DownloadManager>,
    aid: i64,
) -> CommandResult<()> {
    let comic = get_comic(app.clone(), jm_client, aid).await?;
    let chapter_ids: Vec<i64> = comic
        .chapter_infos
        .iter()
        .filter(|chapter_info| chapter_info.is_downloaded != Some(true))
        .map(|chapter_info| chapter_info.chapter_id)
        .collect();
    if chapter_ids.is_empty() {
        let comic_title = comic.name;
        return Err(CommandError::from(
            "一键下载漫画失败",
            anyhow!("漫画`{comic_title}`的所有章节都已存在于下载目录，无需重复下载"),
        ));
    }
    // 创建下载任务前，先创建元数据
    save_metadata(app, comic.clone())?;

    for chapter_id in chapter_ids {
        download_manager
            .create_download_task(comic.clone(), chapter_id)
            .map_err(|err| CommandError::from("一键下载漫画失败", err))?;
    }
    tracing::debug!("一键下载漫画成功，已为所有需要下载的章节创建下载任务");
    Ok(())
}

#[allow(clippy::cast_possible_wrap)]
#[tauri::command(async)]
#[specta::specta]
pub async fn update_downloaded_favorite_comic(
    app: AppHandle,
    jm_client: State<'_, JmClient>,
    download_manager: State<'_, DownloadManager>,
) -> CommandResult<()> {
    let jm_client = jm_client.inner().clone();
    let favorite_comics = Arc::new(Mutex::new(vec![]));
    // 发送正在获取收藏夹事件
    let _ = UpdateDownloadedFavoriteComicEvent::GettingFolders.emit(&app);
    // 获取收藏夹第一页
    let first_page = jm_client
        .get_favorite_folder(0, 1, FavoriteSort::FavoriteTime)
        .await
        .map_err(|err| CommandError::from("更新收藏夹失败", err))?;
    favorite_comics.lock().extend(first_page.list);
    // 计算总页数
    let count = first_page.count;
    let total = first_page
        .total
        .parse::<i64>()
        .map_err(|err| CommandError::from("更新收藏夹失败", err))?;
    let page_count = (total / count) + 1;
    // 获取收藏夹剩余页
    let mut join_set = JoinSet::new();
    for page in 2..=page_count {
        let jm_client = jm_client.clone();
        let favorite_comics = favorite_comics.clone();
        join_set.spawn(async move {
            let page = jm_client
                .get_favorite_folder(0, page, FavoriteSort::FavoriteTime)
                .await?;
            favorite_comics.lock().extend(page.list);
            Ok::<(), anyhow::Error>(())
        });
    }
    // 等待所有请求完成
    while let Some(Ok(get_favorite_result)) = join_set.join_next().await {
        // 如果有请求失败，直接返回错误
        get_favorite_result.map_err(|err| CommandError::from("更新收藏夹失败", err))?;
    }
    // 至此，收藏夹已经全部获取完毕
    let favorite_comics = std::mem::take(&mut *favorite_comics.lock());
    let comics = Arc::new(Mutex::new(vec![]));
    // 限制并发数为10
    let sem = Arc::new(Semaphore::new(10));
    let current = Arc::new(AtomicI64::new(0));
    // 发送正在获取收藏夹漫画详情事件
    let total = favorite_comics.len() as i64;
    let _ = UpdateDownloadedFavoriteComicEvent::GettingComics { total }.emit(&app);
    // 获取收藏夹漫画的详细信息
    for favorite_comic in favorite_comics {
        let sem = sem.clone();
        let aid = favorite_comic
            .id
            .parse::<i64>()
            .map_err(|err| CommandError::from("更新收藏夹失败", err))?;
        let jm_client = jm_client.clone();
        let app = app.clone();
        let comics = comics.clone();
        let current = current.clone();
        join_set.spawn(async move {
            let permit = sem.acquire().await?;
            let comic_resp_data = jm_client.get_comic(aid).await?;
            drop(permit);
            let comic = Comic::from_comic_resp_data(&app, comic_resp_data);
            comics.lock().push(comic);
            let current = current.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            // 发送获取到收藏夹漫画详情事件
            let _ = UpdateDownloadedFavoriteComicEvent::ComicGot { current, total }.emit(&app);
            Ok::<(), anyhow::Error>(())
        });
    }
    // 等待所有请求完成
    while let Some(Ok(get_comic_result)) = join_set.join_next().await {
        // 如果有请求失败，直接返回错误
        get_comic_result.map_err(|err| CommandError::from("更新收藏夹失败", err))?;
    }
    // 至此，收藏夹漫画的详细信息已经全部获取完毕
    let comics = std::mem::take(&mut *comics.lock());
    // 过滤出已下载的漫画(至少有一个章节已下载)
    let downloaded_comics = comics
        .into_iter()
        .filter(|comic| {
            comic
                .chapter_infos
                .iter()
                .any(|chapter_info| chapter_info.is_downloaded == Some(true))
        })
        .collect::<Vec<_>>();
    // 把已下载漫画中的未下载章节添加到下载队列中
    for comic in downloaded_comics {
        let chapter_ids_to_download = comic
            .chapter_infos
            .iter()
            .filter(|chapter| chapter.is_downloaded != Some(true))
            .map(|chapter| chapter.chapter_id);

        for chapter_id in chapter_ids_to_download {
            download_manager
                .create_download_task(comic.clone(), chapter_id)
                .map_err(|err| CommandError::from("更新收藏夹失败", err))?;
        }
    }
    // 发送下载任务创建完成事件
    let _ = UpdateDownloadedFavoriteComicEvent::DownloadTaskCreated.emit(&app);

    Ok(())
}

#[allow(clippy::needless_pass_by_value)]
#[tauri::command(async)]
#[specta::specta]
pub fn show_path_in_file_manager(app: AppHandle, path: &str) -> CommandResult<()> {
    app.opener()
        .reveal_item_in_dir(path)
        .context(format!("在文件管理器中打开`{path}`失败"))
        .map_err(|err| CommandError::from("在文件管理器中打开失败", err))?;
    Ok(())
}

#[allow(clippy::needless_pass_by_value)]
#[tauri::command(async)]
#[specta::specta]
pub fn show_comic_download_dir_in_file_manager(
    app: AppHandle,
    comic_title: String,
) -> CommandResult<()> {
    let comic_download_dir = Comic::get_comic_download_dir(&app, &comic_title);
    app.opener()
        .reveal_item_in_dir(&comic_download_dir)
        .context(format!("在文件管理器中打开`{comic_download_dir:?}`失败"))
        .map_err(|err| CommandError::from("在文件管理器中打开失败", err))?;
    Ok(())
}

#[tauri::command(async)]
#[specta::specta]
pub async fn sync_favorite_folder(jm_client: State<'_, JmClient>) -> CommandResult<()> {
    // 同步收藏夹的方式是随便收藏一个漫画
    // 调用两次toggle是因为要把新收藏的漫画取消收藏
    let task1 = jm_client.toggle_favorite_comic(468_984);
    let task2 = jm_client.toggle_favorite_comic(468_984);
    let (resp1, resp2) =
        tokio::try_join!(task1, task2).map_err(|err| CommandError::from("同步收藏夹失败", err))?;
    if resp1.toggle_type == resp2.toggle_type {
        let toggle_type = resp1.toggle_type;
        let err_title = "同步收藏夹失败";
        let err = anyhow!("两个请求都是`{toggle_type:?}`操作");
        return Err(CommandError::from(err_title, err));
    }

    Ok(())
}

#[allow(clippy::needless_pass_by_value)]
#[tauri::command(async)]
#[specta::specta]
pub fn save_metadata(app: AppHandle, mut comic: Comic) -> CommandResult<()> {
    // 将Comic的is_downloaded字段设置为None，这样能使is_downloaded字段在序列化时被忽略
    comic.is_downloaded = None;
    // 将所有ChapterInfo的is_downloaded字段设置为None，这样能使is_downloaded字段在序列化时被忽略
    for chapter in &mut comic.chapter_infos {
        chapter.is_downloaded = None;
    }

    let comic_title = comic.name.clone();
    let comic_json = serde_json::to_string_pretty(&comic)
        .context(format!(
            "`{comic_title}`的元数据保存失败，将Comic序列化为json失败"
        ))
        .map_err(|err| CommandError::from("保存元数据失败", err))?;
    let comic_download_dir = Comic::get_comic_download_dir(&app, &comic_title);
    let metadata_path = comic_download_dir.join("元数据.json");

    std::fs::create_dir_all(&comic_download_dir)
        .context(format!("创建目录`{comic_download_dir:?}`失败"))
        .map_err(|err| CommandError::from("保存元数据失败", err))?;

    std::fs::write(&metadata_path, comic_json)
        .context(format!("写入文件`{metadata_path:?}`失败"))
        .map_err(|err| CommandError::from("保存元数据失败", err))?;

    Ok(())
}

#[allow(clippy::needless_pass_by_value)]
#[tauri::command(async)]
#[specta::specta]
pub fn get_downloaded_comics(
    app: AppHandle,
    config: State<RwLock<Config>>,
) -> CommandResult<Vec<Comic>> {
    let download_dir = config.read().download_dir.clone();
    // 遍历下载目录，获取所有元数据文件的路径和修改时间
    let mut metadata_path_with_modify_time = std::fs::read_dir(&download_dir)
        .context(format!(
            "获取已下载的漫画失败，读取下载目录`{download_dir:?}`失败"
        ))
        .map_err(|err| CommandError::from("获取已下载的漫画失败", err))?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let metadata_path = entry.path().join("元数据.json");
            if !metadata_path.exists() {
                return None;
            }
            let modify_time = metadata_path.metadata().ok()?.modified().ok()?;
            Some((metadata_path, modify_time))
        })
        .collect::<Vec<_>>();
    // 按照文件修改时间排序，最新的排在最前面
    metadata_path_with_modify_time.sort_by(|(_, a), (_, b)| b.cmp(a));
    let downloaded_comics = metadata_path_with_modify_time
        .iter()
        .filter_map(
            |(metadata_path, _)| match Comic::from_metadata(&app, metadata_path) {
                Ok(comic) => Some(comic),
                Err(err) => {
                    let err_title = format!("读取元数据文件`{metadata_path:?}`失败");
                    let string_chain = err.to_string_chain();
                    tracing::error!(err_title, message = string_chain);
                    None
                }
            },
        )
        .collect::<Vec<_>>();

    Ok(downloaded_comics)
}

#[tauri::command(async)]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn export_cbz(app: AppHandle, comic: Comic) -> CommandResult<()> {
    let comic_title = &comic.name;
    export::cbz(&app, &comic)
        .context(format!("漫画`{comic_title}`导出cbz失败"))
        .map_err(|err| CommandError::from("导出cbz失败", err))?;
    Ok(())
}

#[tauri::command(async)]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn export_pdf(app: AppHandle, comic: Comic) -> CommandResult<()> {
    let comic_title = &comic.name;
    export::pdf(&app, &comic)
        .context(format!("漫画`{comic_title}`导出pdf失败"))
        .map_err(|err| CommandError::from("导出pdf失败", err))?;
    Ok(())
}

#[allow(clippy::needless_pass_by_value)]
#[tauri::command(async)]
#[specta::specta]
pub fn get_logs_dir_size(app: AppHandle) -> CommandResult<u64> {
    let logs_dir = logger::logs_dir(&app)
        .context("获取日志目录失败")
        .map_err(|err| CommandError::from("获取日志目录大小失败", err))?;
    let logs_dir_size = std::fs::read_dir(&logs_dir)
        .context(format!("读取日志目录`{logs_dir:?}`失败"))
        .map_err(|err| CommandError::from("获取日志目录大小失败", err))?
        .filter_map(Result::ok)
        .filter_map(|entry| entry.metadata().ok())
        .map(|metadata| metadata.len())
        .sum::<u64>();
    tracing::debug!("获取日志目录大小成功");
    Ok(logs_dir_size)
}
