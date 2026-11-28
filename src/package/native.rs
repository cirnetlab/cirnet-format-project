use super::{
    Compression, MANIFEST_ENTRY, Operation, PackagePhase, PackageProgress, PackageSource,
    limit_for, members, path,
    read::{Inspected, Stage},
    read_bounded, write,
};
use crate::{
    Digest, Error, Limits, Manifest, ProjectPath, Result,
    error::{check_limit, io},
    integrity::transfer_with,
    storage::{
        Project, open_regular, persist_new, resolve, sync_dir, temp_bytes, trusted_root, write_lock,
    },
};
use std::{
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Read, Seek, Write},
    ops::ControlFlow,
    path::Path,
};
use tempfile::{NamedTempFile, TempDir};

struct NativeSource<'a>(&'a Path);

impl PackageSource for NativeSource<'_> {
    type Reader<'a>
        = File
    where
        Self: 'a;

    fn open(&self, path: &ProjectPath) -> Result<File> {
        open_regular(self.0, path)
    }
}

/// Write a deterministic ZIP to an empty seekable sink under the project's writer lock.
/// On failure discard the sink.
pub fn export<W: Write + Seek>(sink: W, project: &Project, compression: Compression) -> Result<W> {
    export_with(
        sink,
        project,
        compression,
        &mut Operation::quiet(PackagePhase::Export),
    )
}

/// The callback runs under the writer lock during export. It must not start a
/// second write or export on the same project.
pub fn export_with_progress<W: Write + Seek>(
    sink: W,
    project: &Project,
    compression: Compression,
    mut progress: impl FnMut(PackageProgress<'_>) -> ControlFlow<()>,
) -> Result<W> {
    export_with(
        sink,
        project,
        compression,
        &mut Operation::new(PackagePhase::Export, &mut progress),
    )
}

fn export_with<W: Write + Seek>(
    mut sink: W,
    project: &Project,
    compression: Compression,
    operation: &mut Operation<'_>,
) -> Result<W> {
    write::check_empty_sink(&mut sink)?;
    operation.check(None, 0)?;
    let _lock = write_lock(project.root())?;
    project.ensure_no_pending()?;
    project.check_revision()?;
    let sink = write::export_into_empty(
        sink,
        project.manifest(),
        &NativeSource(project.root()),
        compression,
        project.limits(),
        operation,
    )?;
    project.check_revision()?;
    Ok(sink)
}

/// Atomically publish a new package without replacing an existing file.
pub fn export_file(
    path: impl AsRef<Path>,
    project: &Project,
    compression: Compression,
) -> Result<()> {
    export_file_with(
        path.as_ref(),
        project,
        compression,
        &mut Operation::quiet(PackagePhase::Export),
    )
}

/// Export to a new file with cooperative cancellation. The callback must not
/// start another write or export on the same project.
pub fn export_file_with_progress(
    path: impl AsRef<Path>,
    project: &Project,
    compression: Compression,
    mut progress: impl FnMut(PackageProgress<'_>) -> ControlFlow<()>,
) -> Result<()> {
    export_file_with(
        path.as_ref(),
        project,
        compression,
        &mut Operation::new(PackagePhase::Export, &mut progress),
    )
}

fn export_file_with(
    path: &Path,
    project: &Project,
    compression: Compression,
    operation: &mut Operation<'_>,
) -> Result<()> {
    let (parent, name) = destination_parts(path)?;
    if name.extension() != Some("cirpkg") {
        return Err(Error::Path(name.to_string()));
    }
    let parent = trusted_root(parent)?;
    let mut temp = NamedTempFile::new_in(&parent).map_err(|error| io(name.as_str(), error))?;
    export_with(temp.as_file_mut(), project, compression, operation)?;
    temp.as_file()
        .sync_all()
        .map_err(|error| io(name.as_str(), error))?;
    operation.check(None, 0)?;
    persist_new(temp, &parent.join(name.as_path()))
}

fn destination_parts(destination: &Path) -> Result<(&Path, ProjectPath)> {
    let name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| Error::Path(destination.display().to_string()))?;
    let name = ProjectPath::new(name)?;
    let parent = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    Ok((parent, name))
}

struct DirectoryStage(TempDir);

impl Stage for DirectoryStage {
    type Writer = File;

    fn create(&mut self, name: &ProjectPath) -> Result<File> {
        let destination = resolve(self.0.path(), name, true)?;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options
            .open(destination)
            .map_err(|error| io(name.as_str(), error))
    }

    fn finish(&mut self, name: &ProjectPath, writer: File) -> Result<()> {
        writer.sync_all().map_err(|error| io(name.as_str(), error))
    }
}

pub struct VerifiedPackage {
    directory: TempDir,
    manifest: Manifest,
    limits: Limits,
}

/// Validate every member into private scratch storage. No import destination is opened here.
/// The scratch parent must be a local directory controlled by the caller.
pub fn verify<R: Read + Seek>(
    reader: R,
    scratch_parent: impl AsRef<Path>,
    limits: Limits,
) -> Result<VerifiedPackage> {
    verify_with(
        reader,
        scratch_parent.as_ref(),
        limits,
        &mut Operation::quiet(PackagePhase::Verify),
    )
}

/// Verify into scratch storage with cooperative cancellation. Scratch is removed on failure.
pub fn verify_with_progress<R: Read + Seek>(
    reader: R,
    scratch_parent: impl AsRef<Path>,
    limits: Limits,
    mut progress: impl FnMut(PackageProgress<'_>) -> ControlFlow<()>,
) -> Result<VerifiedPackage> {
    verify_with(
        reader,
        scratch_parent.as_ref(),
        limits,
        &mut Operation::new(PackagePhase::Verify, &mut progress),
    )
}

fn verify_with<R: Read + Seek>(
    reader: R,
    scratch_parent: &Path,
    limits: Limits,
    operation: &mut Operation<'_>,
) -> Result<VerifiedPackage> {
    let inspected = Inspected::open(reader, &limits, operation)?;
    let mut builder = tempfile::Builder::new();
    builder.prefix(".cirnet-verify-");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(fs::Permissions::from_mode(0o700));
    }
    let mut stage = DirectoryStage(
        builder
            .tempdir_in(trusted_root(scratch_parent)?)
            .map_err(|error| io("private package stage", error))?,
    );
    let manifest = inspected.verify(&limits, &mut stage, operation)?;
    Ok(VerifiedPackage {
        directory: stage.0,
        manifest,
        limits,
    })
}

impl VerifiedPackage {
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Publish to a new directory. The manifest is installed after all its resources.
    /// A destination on another volume is supported by bounded copies.
    pub fn publish(self, destination: impl AsRef<Path>) -> Result<Project> {
        self.publish_with(
            destination.as_ref(),
            &mut Operation::quiet(PackagePhase::Publish),
        )
    }

    /// An interruption before manifest publication can leave unreferenced files and directories.
    pub fn publish_with_progress(
        self,
        destination: impl AsRef<Path>,
        mut progress: impl FnMut(PackageProgress<'_>) -> ControlFlow<()>,
    ) -> Result<Project> {
        self.publish_with(
            destination.as_ref(),
            &mut Operation::new(PackagePhase::Publish, &mut progress),
        )
    }

    fn publish_with(self, destination: &Path, operation: &mut Operation<'_>) -> Result<Project> {
        operation.check(None, 0)?;
        let (parent, name) = destination_parts(destination)?;
        let parent = trusted_root(parent)?;
        let root = parent.join(name.as_path());
        fs::create_dir(&root).map_err(|error| {
            if error.kind() == ErrorKind::AlreadyExists {
                Error::Conflict(root.display().to_string())
            } else {
                io(root.display().to_string(), error)
            }
        })?;
        sync_dir(&parent)?;
        let manifest_name = path(MANIFEST_ENTRY);
        for (name, doc) in members(&self.manifest)? {
            if name != manifest_name {
                self.publish_resource(&root, &name, doc.is_some(), operation)?;
            }
        }
        operation.check(Some(MANIFEST_ENTRY), 0)?;
        let bytes = read_bounded(
            open_regular(self.directory.path(), &manifest_name)?,
            self.limits.max_json_bytes,
            MANIFEST_ENTRY,
            |bytes| operation.check(Some(MANIFEST_ENTRY), bytes),
        )?;
        let file = temp_bytes(&root, &bytes)?;
        let revision = Digest::of(&bytes);
        let target = root.join(manifest_name.as_path());
        operation.check(None, 0)?;
        persist_new(file, &target)?;
        Ok(Project::from_parts(
            root,
            manifest_name,
            self.manifest,
            revision,
            self.limits,
        ))
    }

    fn publish_resource(
        &self,
        root: &Path,
        name: &ProjectPath,
        document: bool,
        operation: &mut Operation<'_>,
    ) -> Result<()> {
        let context = name.as_str();
        operation.check(Some(context), 0)?;
        let target = resolve(root, name, true)?;
        let parent = target.parent().expect("target has parent");
        let staged = open_regular(self.directory.path(), name)?;
        let limit = limit_for(name, document, &self.limits);
        let len = staged.metadata().map_err(|error| io(context, error))?.len();
        check_limit("stream bytes", len, limit)?;

        // Windows files can preserve source ACLs when linked. Keep that fast path
        // within the same trusted parent; other destinations get locally created files.
        let stage = self.directory.path();
        let link = cfg!(unix) || stage.parent() == root.parent();
        if link && link_staged(&stage.join(name.as_path()), &target, parent)? {
            return operation.check(Some(context), len);
        }

        let mut temp = NamedTempFile::new_in(parent).map_err(|error| io(context, error))?;
        transfer_with(staged, temp.as_file_mut(), limit, context, |bytes| {
            operation.check(Some(context), bytes)
        })?;
        temp.as_file()
            .sync_all()
            .map_err(|error| io(context, error))?;
        persist_new(temp, &target)
    }
}

// Linking avoids copying verified, file-synced bytes and refuses to replace a file.
// Cross-volume and unsupported links fall back to the bounded local-copy path.
fn link_staged(source: &Path, target: &Path, parent: &Path) -> Result<bool> {
    match fs::hard_link(source, target) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {
            return Err(Error::Conflict(target.display().to_string()));
        }
        Err(_) => return Ok(false),
    }
    sync_dir(parent).map_err(|error| match error {
        Error::Io { source, .. } => Error::Durability {
            context: target.display().to_string(),
            source,
        },
        other => other,
    })?;
    Ok(true)
}
